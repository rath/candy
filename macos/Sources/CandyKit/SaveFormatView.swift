import AppKit
import UniformTypeIdentifiers

enum ExportFormat: Int, CaseIterable {
  case png, jpeg, webp
  var title: String { ["PNG", "JPEG", "WebP (lossless)"][rawValue] }
  var type: UTType { [.png, .jpeg, .webP][rawValue] }
  var extensionName: String { ["png", "jpg", "webp"][rawValue] }
  static func forFilename(_ name: String) -> ExportFormat {
    switch (name as NSString).pathExtension.lowercased() {
    case "jpg", "jpeg": return .jpeg
    case "webp": return .webp
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
    formats.addItems(withTitles: ExportFormat.allCases.map(\.title))
    formats.selectItem(at: ExportFormat.forFilename(panel.nameFieldStringValue).rawValue)
    formats.target = self
    formats.action = #selector(changeFormat)
    formats.setAccessibilityLabel("Export format")
    addArrangedSubview(formats)
    changeFormat()
  }
  required init?(coder: NSCoder) { fatalError("Programmatic accessory") }
  @objc private func changeFormat() {
    guard let panel, let format = ExportFormat(rawValue: formats.indexOfSelectedItem) else {
      return
    }
    panel.allowedContentTypes = [format.type]
    panel.nameFieldStringValue = format.filename(panel.nameFieldStringValue)
  }
}
