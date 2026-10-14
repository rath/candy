import AppKit
import Testing

@testable import CandyKit

@Suite(.serialized) @MainActor struct SaveOptionsTests {
  private func controls<T: NSView>(_ view: NSView, of type: T.Type) -> [T] {
    (view as? T).map { [$0] } ?? view.subviews.flatMap { controls($0, of: type) }
  }

  @Test func savePanelSwitchesOptionsAndKeepsItsDraftSeparate() throws {
    _ = NSApplication.shared
    let panel = NSSavePanel()
    panel.nameFieldStringValue = "photo.jpg"
    let original = SaveSettings()
    let view = SaveFormatView(panel: panel, settings: original)
    panel.accessoryView = view
    let popups = controls(view, of: NSPopUpButton.self)
    let formats = try #require(popups.first { $0.numberOfItems > 2 })
    let compression = try #require(popups.first { $0.numberOfItems == 2 })
    let field = try #require(controls(view, of: NSTextField.self).first { $0.isEditable })
    let slider = try #require(controls(view, of: NSSlider.self).first)
    func select(_ popup: NSPopUpButton, _ index: Int) {
      popup.selectItem(at: index)
      _ = view.perform(popup.action)
    }
    func enter(_ text: String) {
      field.stringValue = text
      view.controlTextDidChange(
        Notification(name: NSControl.textDidChangeNotification, object: field))
    }
    #expect(field.integerValue == 92)
    enter("73")
    #expect(slider.integerValue == 73)
    #expect(view.settings.options(for: .jpeg).quality == 73)
    for invalid in ["", "0", "101", "3.5", "abc"] {
      enter(invalid)
      #expect(throws: (any Error).self) {
        try view.panel(panel, validate: URL(fileURLWithPath: "/tmp/photo.jpg"))
      }
    }
    enter("73")
    try view.panel(panel, validate: URL(fileURLWithPath: "/tmp/photo.jpg"))
    select(formats, try #require(ExportFormat.available.firstIndex(of: .webp)))
    #expect(panel.nameFieldStringValue == "photo.webp")
    #expect(field.superview?.isHidden == true)
    #expect(view.settings.options(for: .webp) == SaveOptions(webpMode: .lossless))
    select(compression, 1)
    #expect(field.superview?.isHidden == false)
    #expect(field.integerValue == 80)
    enter("64")
    select(compression, 0)
    #expect(view.settings.options(for: .webp).quality == nil)
    select(compression, 1)
    #expect(field.integerValue == 64)
    select(formats, try #require(ExportFormat.available.firstIndex(of: .jpeg)))
    #expect(field.integerValue == 73)
    slider.doubleValue = 41.6
    _ = view.perform(slider.action)
    #expect(field.integerValue == 42)
    select(formats, try #require(ExportFormat.available.firstIndex(of: .png)))
    #expect(field.superview?.isHidden == true)
    #expect(compression.superview?.isHidden == true)
    #expect(view.settings.options(for: .png) == SaveOptions())
    // Cancelling discards this value snapshot; the caller's settings never changed.
    #expect(original == SaveSettings())
    let reopened = SaveFormatView(panel: panel, settings: view.settings)
    #expect(reopened.settings.quality(for: .jpeg) == 42)
    #expect(reopened.settings.quality(for: .webp) == 64)
  }

  @Test func qualityDefaultsAndCValuesAreValidated() throws {
    let settings = SaveSettings()
    #expect(settings.options(for: .jpeg).quality == 92)
    for format in [ExportFormat.heic, .avif, .jpeg2000] {
      #expect(settings.options(for: format).quality == 85)
    }
    #expect(try SaveOptions().cValue().quality == 0)
    #expect(try SaveOptions(quality: 1).cValue().quality == 1)
    #expect(try SaveOptions(quality: 100, webpMode: .lossy).cValue().webp_mode == 2)
    for invalid in [-1, 0, 101, 256] {
      #expect(throws: (any Error).self) { _ = try SaveOptions(quality: invalid).cValue() }
    }
  }
}
