import CCandy
import CoreGraphics
import Foundation

struct EngineError: LocalizedError {
  let message: String
  var errorDescription: String? { message }
}
private func checked<T>(_ body: (UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> T) throws
  -> T
{
  var error: UnsafeMutablePointer<CChar>?
  let result = body(&error)
  if let error {
    defer { candy_error_free(error) }
    throw EngineError(message: String(cString: error))
  }
  return result
}

struct PixelRect: Equatable, Sendable {
  var x: UInt32, y: UInt32, width: UInt32, height: UInt32
  var cValue: CandyRect { CandyRect(x: x, y: y, width: width, height: height) }
  var cgRect: CGRect { CGRect(x: Int(x), y: Int(y), width: Int(width), height: Int(height)) }
  static func between(_ a: CGPoint, _ b: CGPoint, size: CGSize) -> PixelRect? {
    guard a.x.isFinite, a.y.isFinite, b.x.isFinite, b.y.isFinite else { return nil }
    let x = min(size.width, max(0, floor(min(a.x, b.x))))
    let y = min(size.height, max(0, floor(min(a.y, b.y))))
    let right = min(size.width, max(0, ceil(max(a.x, b.x))))
    let bottom = min(size.height, max(0, ceil(max(a.y, b.y))))
    guard right > x, bottom > y else { return nil }
    return PixelRect(
      x: UInt32(x), y: UInt32(y), width: UInt32(right - x), height: UInt32(bottom - y))
  }
}
struct DocumentState: Sendable {
  let width: Int, height: Int
  let stateID: UInt64, revision: UInt64
  let dirty: Bool, canUndo: Bool, canRedo: Bool
  var size: CGSize { CGSize(width: width, height: height) }
}
struct Preview: Sendable {
  let width: Int, height: Int
  let bytes: Data
  func image() -> CGImage? {
    guard let provider = CGDataProvider(data: bytes as CFData),
      let space = CGColorSpace(name: CGColorSpace.sRGB)
    else { return nil }
    return CGImage(
      width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
      bytesPerRow: width * 4, space: space,
      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue).union(
        .byteOrder32Big),
      provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
  }
}
struct RenderedDocument: Sendable {
  let state: DocumentState
  let preview: Preview
}
enum Edit: Sendable {
  case resize(UInt32, UInt32)
  case flatten(UInt8, UInt8, UInt8)
  case mosaic([PixelRect], UInt32)
  case highlight([CGPoint], Float, [UInt8])
  case undo, redo
}

// Access is restricted to a DocumentSession's serial worker. A queued closure
// retains this owner until its call finishes, including after a window closes.
final class RustDocument: @unchecked Sendable {
  private let handle: OpaquePointer
  private init(_ handle: OpaquePointer) { self.handle = handle }
  deinit { candy_document_free(handle) }
  static func open(_ url: URL) throws -> RustDocument {
    let handle = try url.withUnsafeFileSystemRepresentation { path in
      try checked { candy_document_open(path, $0) }
    }
    guard let handle else { throw EngineError(message: "Cannot open image") }
    return RustDocument(handle)
  }
  func state() throws -> DocumentState {
    let s = try checked { candy_document_state(handle, $0) }
    return DocumentState(
      width: Int(s.width), height: Int(s.height), stateID: s.state_id,
      revision: s.revision, dirty: s.dirty != 0, canUndo: s.can_undo != 0, canRedo: s.can_redo != 0)
  }
  func render() throws -> RenderedDocument {
    guard let p = try checked({ candy_document_preview(handle, $0) }) else {
      throw EngineError(message: "Cannot render image")
    }
    defer { candy_preview_free(p) }
    let preview = Preview(
      width: Int(p.pointee.width), height: Int(p.pointee.height),
      bytes: Data(bytes: p.pointee.data, count: p.pointee.length))
    return try RenderedDocument(state: state(), preview: preview)
  }
  func apply(_ edit: Edit) throws {
    let _: UInt8 = try checked { error in
      switch edit {
      case .resize(let w, let h): return candy_document_resize(handle, w, h, error)
      case .flatten(let r, let g, let b): return candy_document_flatten(handle, r, g, b, error)
      case .mosaic(let rects, let block):
        return rects.map(\.cValue).withUnsafeBufferPointer {
          candy_document_mosaic(handle, $0.baseAddress, $0.count, block, error)
        }
      case .highlight(let points, let width, let color):
        return points.map { CandyPoint(x: Float($0.x), y: Float($0.y)) }.withUnsafeBufferPointer {
          candy_document_highlight(
            handle, $0.baseAddress, $0.count, width, color[0], color[1], color[2], color[3], error)
        }
      case .undo: return candy_document_history(handle, 0, error)
      case .redo: return candy_document_history(handle, 1, error)
      }
    }
  }
  func extract(_ rect: PixelRect) throws -> RustDocument {
    guard let new = try checked({ candy_document_extract(handle, rect.cValue, $0) }) else {
      throw EngineError(message: "Cannot extract selection")
    }
    return RustDocument(new)
  }
  func save(_ url: URL) throws -> DocumentState {
    let _: UInt8 = try url.withUnsafeFileSystemRepresentation { path in
      try checked { candy_document_save(handle, path, $0) }
    }
    return try state()
  }
  func snapshot() throws -> RustImage {
    guard let image = try checked({ candy_document_snapshot(handle, $0) }) else {
      throw EngineError(message: "Cannot copy image")
    }
    return RustImage(image)
  }
}
final class RustImage: @unchecked Sendable {
  private let handle: OpaquePointer
  init(_ handle: OpaquePointer) { self.handle = handle }
  deinit { candy_image_free(handle) }
  func detect() throws -> [PixelRect] {
    guard let result = try checked({ candy_image_detect(handle, $0) }) else {
      throw EngineError(message: "Cannot detect text")
    }
    defer { candy_regions_free(result) }
    return UnsafeBufferPointer(start: result.pointee.data, count: result.pointee.count).map {
      PixelRect(x: $0.x, y: $0.y, width: $0.width, height: $0.height)
    }
  }
}
