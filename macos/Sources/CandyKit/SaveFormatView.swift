import AppKit
import ImageIO
import UniformTypeIdentifiers

/// Export formats, in popup order. Keep the extensions in step with
/// `candy_macos::save` and `candy_core::export_format`, which do the encoding.
enum ExportFormat: Int, CaseIterable, Sendable {
  case png, jpeg, heic, webp, avif, tiff, gif, bmp, jpeg2000, tga
  var title: String {
    [
      "PNG", "JPEG", "HEIC", "WebP", "AVIF", "TIFF", "GIF", "BMP",
      "JPEG 2000",
      "TGA",
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
  var supportsQuality: Bool {
    switch self {
    case .jpeg, .heic, .webp, .avif, .jpeg2000: return true
    default: return false
    }
  }
  var defaultQuality: Int {
    switch self {
    case .jpeg: return 92
    case .webp: return 80
    default: return 85
    }
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

/// A value snapshot: a save sheet edits its own copy and the session commits it
/// only after a successful save. Nothing is persisted across documents.
struct SaveSettings: Equatable, Sendable {
  var qualities: [ExportFormat: Int] = [:]
  var webpMode: WebpMode = .lossless

  func quality(for format: ExportFormat) -> Int {
    qualities[format] ?? format.defaultQuality
  }
  func showsQuality(for format: ExportFormat) -> Bool {
    format.supportsQuality && (format != .webp || webpMode == .lossy)
  }
  func options(for format: ExportFormat) -> SaveOptions {
    SaveOptions(
      quality: showsQuality(for: format) ? quality(for: format) : nil,
      webpMode: format == .webp ? webpMode : nil)
  }
}

@MainActor final class SaveFormatView: NSStackView, NSTextFieldDelegate, NSOpenSavePanelDelegate {
  private weak var panel: NSSavePanel?
  private let formats = NSPopUpButton()
  private let compression = NSPopUpButton()
  private let quality = NSSlider(value: 92, minValue: 1, maxValue: 100, target: nil, action: nil)
  private let qualityField = NSTextField(string: "92")
  private let compressionRow = NSStackView()
  private let qualityRow = NSStackView()
  private let validationMessage = NSTextField(
    labelWithString: localized("Quality must be an integer from 1 to 100."))
  private(set) var settings: SaveSettings
  var selectedFormat: ExportFormat {
    ExportFormat.available[formats.indexOfSelectedItem]
  }

  init(panel: NSSavePanel, settings: SaveSettings = SaveSettings()) {
    self.panel = panel
    self.settings = settings
    super.init(frame: NSRect(x: 0, y: 0, width: 380, height: 110))
    orientation = .vertical
    alignment = .leading
    spacing = 8
    edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    let formatRow = NSStackView()
    configureRow(formatRow, label: localized("Format:"))
    formats.addItems(withTitles: ExportFormat.available.map(\.title))
    let initial = ExportFormat.forFilename(panel.nameFieldStringValue)
    formats.selectItem(at: ExportFormat.available.firstIndex(of: initial) ?? 0)
    formats.target = self
    formats.action = #selector(changeFormat)
    formats.setAccessibilityLabel(localized("Export format"))
    formatRow.addArrangedSubview(formats)
    configureRow(compressionRow, label: localized("Compression:"))
    compression.addItems(withTitles: [localized("Lossless"), localized("Lossy")])
    compression.target = self
    compression.action = #selector(changeCompression)
    compression.setAccessibilityLabel(localized("WebP compression"))
    compressionRow.addArrangedSubview(compression)
    configureRow(qualityRow, label: localized("Quality:"))
    quality.target = self
    quality.action = #selector(changeQuality)
    quality.isContinuous = true
    quality.widthAnchor.constraint(equalToConstant: 160).isActive = true
    quality.setAccessibilityLabel(localized("Export quality"))
    qualityField.widthAnchor.constraint(equalToConstant: 48).isActive = true
    qualityField.alignment = .right
    qualityField.font = .monospacedDigitSystemFont(ofSize: 12, weight: .regular)
    qualityField.delegate = self
    qualityField.setAccessibilityLabel(localized("Export quality"))
    qualityField.toolTip = localized("Quality must be an integer from 1 to 100.")
    qualityRow.addArrangedSubview(quality)
    qualityRow.addArrangedSubview(qualityField)
    validationMessage.textColor = .systemRed
    validationMessage.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    addArrangedSubview(validationMessage)
    panel.delegate = self
    changeFormat()
  }
  required init?(coder: NSCoder) { fatalError("Programmatic accessory") }

  private func configureRow(_ row: NSStackView, label: String) {
    row.orientation = .horizontal
    row.alignment = .centerY
    row.spacing = 10
    let title = NSTextField(labelWithString: label)
    title.alignment = .right
    row.addArrangedSubview(title)
    addArrangedSubview(row)
    // Equal label widths allow each language to use its natural text width.
    if let first = arrangedSubviews.first as? NSStackView,
      let firstLabel = first.arrangedSubviews.first, firstLabel !== title
    {
      title.widthAnchor.constraint(equalTo: firstLabel.widthAnchor).isActive = true
    }
  }

  private func refreshOptions() {
    let format = selectedFormat
    compression.selectItem(at: settings.webpMode == .lossless ? 0 : 1)
    compressionRow.isHidden = format != .webp
    qualityRow.isHidden = !settings.showsQuality(for: format)
    quality.integerValue = settings.quality(for: format)
    qualityField.integerValue = quality.integerValue
    validationMessage.isHidden = true
    qualityField.textColor = .labelColor
  }

  @objc private func changeFormat() {
    guard let panel, ExportFormat.available.indices.contains(formats.indexOfSelectedItem) else {
      return
    }
    let format = ExportFormat.available[formats.indexOfSelectedItem]
    panel.allowedContentTypes = [format.type]
    panel.nameFieldStringValue = format.filename(panel.nameFieldStringValue)
    refreshOptions()
  }

  @objc private func changeCompression() {
    settings.webpMode = compression.indexOfSelectedItem == 0 ? .lossless : .lossy
    refreshOptions()
  }

  @objc private func changeQuality() {
    let value = Int(quality.doubleValue.rounded())
    settings.qualities[selectedFormat] = value
    quality.integerValue = value
    qualityField.integerValue = value
    qualityField.textColor = .labelColor
    validationMessage.isHidden = true
  }

  private var enteredQuality: Int? {
    guard let value = Int(qualityField.stringValue), (1...100).contains(value) else { return nil }
    return value
  }

  func controlTextDidChange(_ notification: Notification) {
    let value = enteredQuality
    validationMessage.isHidden = value != nil
    qualityField.textColor = value == nil ? .systemRed : .labelColor
    if let value {
      settings.qualities[selectedFormat] = value
      quality.integerValue = value
    }
  }

  func panel(_ sender: Any, validate url: URL) throws {
    if settings.showsQuality(for: selectedFormat), enteredQuality == nil {
      throw EngineError(message: localized("Quality must be an integer from 1 to 100."))
    }
  }
}
