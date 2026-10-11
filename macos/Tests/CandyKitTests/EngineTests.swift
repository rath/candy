import AppKit
import Testing

@testable import CandyKit

private var fixture: URL {
  URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(
      "crates/candy-macos/tests/fixtures/sample.png")
}

@Test func engineEditsAndSavesThroughCABI() throws {
  let d = try RustDocument.open(fixture)
  #expect(try d.state().width == 1000)
  #expect(try !d.state().dirty)
  let initial = try d.render().preview.bytes
  try d.apply(.mosaic([PixelRect(x: 40, y: 40, width: 100, height: 100)], 12))
  #expect(try d.render().preview.bytes != initial)
  try d.apply(.undo)
  #expect(try d.render().preview.bytes == initial)
  #expect(try !d.state().dirty)
  try d.apply(.highlight([CGPoint(x: 20, y: 20), CGPoint(x: 70, y: 70)], 12, [255, 0, 0, 100]))
  #expect(try d.state().canRedo == false)
  let crop = try d.extract(PixelRect(x: 0, y: 0, width: 50, height: 60))
  #expect(try crop.state().dirty)
  #expect(try crop.state().width == 50)
  try crop.apply(.resize(25, 30))
  try crop.apply(.flatten(255, 255, 255))
  let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
  defer { try? FileManager.default.removeItem(at: folder) }
  for ext in ["png", "jpg", "webp"] {
    let out = folder.appendingPathComponent("한글 copy.\(ext)")
    #expect(try !crop.save(out).dirty)
    #expect(try RustDocument.open(out).state().height == 30)
  }
  #expect(throws: (any Error).self) { _ = try crop.save(fixture) }
}
@Test func failedEditsPreserveStateAndPreview() throws {
  let d = try RustDocument.open(fixture)
  let before = try d.render()
  #expect(throws: (any Error).self) { try d.apply(.resize(0, 50)) }
  let after = try d.render()
  #expect(before.state.revision == after.state.revision)
  #expect(before.preview.bytes == after.preview.bytes)
}
@Test func transformsAndSelectionUseImageCoordinates() {
  let transform = CanvasTransform(origin: CGPoint(x: 31, y: -70), scale: 0.37)
  let p = CGPoint(x: 980, y: 504)
  let result = transform.image(transform.screen(p))
  #expect(abs(result.x - p.x) < 0.0001 && abs(result.y - p.y) < 0.0001)
  #expect(
    PixelRect.between(
      CGPoint(x: 100, y: 90), CGPoint(x: -10, y: 20), size: CGSize(width: 200, height: 150))
      == PixelRect(x: 0, y: 20, width: 100, height: 70))
  #expect(PixelRect.between(.zero, .zero, size: CGSize(width: 20, height: 20)) == nil)
  var regions = Regions(boxes: [PixelRect(x: 1, y: 1, width: 2, height: 2)], active: 9)
  regions.removeActive()
  #expect(regions.active == nil && regions.boxes.count == 1)
}

@Suite(.serialized) @MainActor struct SessionTests {
  private func loaded() async throws -> DocumentSession {
    let s = DocumentSession()
    s.load(fixture)
    try await settled(s)
    #expect(s.state != nil)
    return s
  }
  private func settled(_ s: DocumentSession) async throws {
    for _ in 0..<500 {
      if !s.busy { return }
      try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("Document operation did not finish")
  }
  @Test func staleDetectionAndClosedDocumentCannotPublish() async throws {
    let s = try await loaded()
    var received = false
    s.onRegions = { _ in received = true }
    let old = s.generation
    s.apply(.resize(200, 132))
    s.acceptDetection(.success([PixelRect(x: 1, y: 1, width: 2, height: 2)]), generation: old)
    #expect(!received)
    try await settled(s)
    s.invalidate()
    s.acceptDetection(.success([]), generation: s.generation)
    #expect(!received)
  }
  @Test func savingFailureKeepsDirtyAndDoesNotChooseOutput() async throws {
    let s = try await loaded()
    s.apply(.resize(200, 132))
    try await settled(s)
    #expect(s.dirty)
    let saved = await withCheckedContinuation { continuation in
      s.save(fixture) { continuation.resume(returning: $0) }
    }
    #expect(!saved && s.dirty && s.output == nil)
  }
  @Test func actualSizeUsesBackingPixels() {
    let canvas = CanvasView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
    canvas.imageSize = CGSize(width: 1000, height: 660)
    canvas.actualSize()
    #expect(canvas.transform.scale == 0.5)
    #expect(canvas.zoomPercent == 100)
  }
}
