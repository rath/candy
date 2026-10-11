import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Export formats, in popup order. Keep the extensions in step with
/// `candy_macos::save` and `candy_core::export_format`, which do the encoding.
enum ExportFormat: Int, CaseIterable {
  case png, jpeg, heic, webp, avif, tiff, gif, bmp, jpeg2000, tga
  var title: String {
    [
      "PNG", "JPEG", "HEIC", "WebP (lossless)", "AVIF", "TIFF", "GIF", "BMP", "JPEG 2000", "TGA",
    ][rawValue]
  }
  var identifier: String {
    [
      "public.png", "public.jpeg", "public.heic", "org.webmproject.webp", "public.avif",
      "public.tiff", "com.compuserve.gif", "com.microsoft.bmp", "public.jpeg-2000",
      "com.truevision.tga-image",
    ][rawValue]
  }
  var type: UTType { UTType(identifier) ?? .image }
  var extensionName: String {
    ["png", "jpg", "heic", "webp", "avif", "tif", "gif", "bmp", "jp2", "tga"][rawValue]
  }
  /// Formats this macOS version can write. Image I/O gained AVIF encoding
  /// after macOS 14, so AVIF appears only where it works.
  static let available: [ExportFormat] = {
    let encodable = CGImageDestinationCopyTypeIdentifiers() as? [String] ?? []
    return allCases.filter { $0 != .avif || encodable.contains($0.identifier) }
  }()
  static func forFilename(_ name: String) -> ExportFormat {
    switch (name as NSString).pathExtension.lowercased() {
    case "jpg", "jpeg": return .jpeg
    case "heic", "heif": return .heic
    case "webp": return .webp
    case "avif": return .avif
    case "tif", "tiff": return .tiff
    case "gif": return .gif
    case "bmp": return .bmp
    case "jp2", "j2k", "jpf", "jpx": return .jpeg2000
    case "tga": return .tga
    default: return .png
    }
  }
  func filename(_ name: String) -> String {
    let stem = (name as NSString).deletingPathExtension
    return (stem.isEmpty ? "image" : stem) + "." + extensionName
  }
}

@MainActor final class SaveFormatView: NSStackView {
  private weak var panel: NSSavePanel?
  private let formats = NSPopUpButton()
  init(panel: NSSavePanel) {
    self.panel = panel
    super.init(frame: NSRect(x: 0, y: 0, width: 300, height: 38))
    orientation = .horizontal
    alignment = .centerY
    spacing = 10
    edgeInsets = NSEdgeInsets(top: 6, left: 12, bottom: 6, right: 12)
    addArrangedSubview(NSTextField(labelWithString: "Format:"))
    formats.addItems(withTitles: ExportFormat.available.map(\.title))
    let initial = ExportFormat.forFilename(panel.nameFieldStringValue)
    formats.selectItem(at: ExportFormat.available.firstIndex(of: initial) ?? 0)
    formats.target = self
    formats.action = #selector(changeFormat)
    formats.setAccessibilityLabel("Export format")
    addArrangedSubview(formats)
    changeFormat()
  }
  required init?(coder: NSCoder) { fatalError("Programmatic accessory") }
  @objc private func changeFormat() {
    guard let panel, ExportFormat.available.indices.contains(formats.indexOfSelectedItem) else {
      return
    }
    let format = ExportFormat.available[formats.indexOfSelectedItem]
    panel.allowedContentTypes = [format.type]
    panel.nameFieldStringValue = format.filename(panel.nameFieldStringValue)
  }
}
