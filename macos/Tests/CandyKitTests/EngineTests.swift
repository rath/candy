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
  try crop.apply(.rotate(clockwise: true))
  #expect(try crop.state().width == 30 && crop.state().height == 25)
  try crop.apply(.rotate(clockwise: false))
  #expect(try crop.state().width == 25 && crop.state().height == 30)
  try crop.apply(.flatten(255, 255, 255))
  let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
  defer { try? FileManager.default.removeItem(at: folder) }
  let avif = ExportFormat.available.contains(.avif) ? ["avif"] : []
  for ext in ["png", "jpg", "webp", "heic", "tif", "gif", "bmp", "jp2", "tga"] + avif {
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
  @Test func successfulSavesRememberOptionsOnlyInTheirDocument() async throws {
    let s = try await loaded()
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: folder) }
    let output = folder.appendingPathComponent("copy.jpg")
    var settings = SaveSettings()
    settings.qualities[.jpeg] = 23
    settings.qualities[.webp] = 67
    settings.webpMode = .lossy
    let first = await withCheckedContinuation { continuation in
      s.save(output, settings: settings) { continuation.resume(returning: $0) }
    }
    #expect(first && s.saveSettings == settings)
    let firstBytes = try Data(contentsOf: output)
    let again = await withCheckedContinuation { continuation in
      s.save(output) { continuation.resume(returning: $0) }
    }
    #expect(again && s.saveSettings == settings)
    #expect(try Data(contentsOf: output) == firstBytes)
    let reference = try RustDocument.open(fixture)
    let expected = folder.appendingPathComponent("expected.jpg")
    _ = try reference.save(expected, options: SaveOptions(quality: 23))
    #expect(try Data(contentsOf: expected) == firstBytes)

    s.apply(.rotate(clockwise: true))
    try await settled(s)
    var draft = settings
    draft.qualities[.jpeg] = 99
    draft.webpMode = .lossless
    let failed = await withCheckedContinuation { continuation in
      s.save(fixture, settings: draft) { continuation.resume(returning: $0) }
    }
    #expect(!failed && s.dirty && s.output == output && s.saveSettings == settings)
    let copy = folder.appendingPathComponent("copy.webp")
    let savedAs = await withCheckedContinuation { continuation in
      s.save(copy) { continuation.resume(returning: $0) }
    }
    #expect(savedAs && s.output == copy && s.saveSettings == settings)
    try reference.apply(.rotate(clockwise: true))
    let expectedWebp = folder.appendingPathComponent("expected.webp")
    _ = try reference.save(expectedWebp, options: SaveOptions(quality: 67, webpMode: .lossy))
    #expect(try Data(contentsOf: copy) == Data(contentsOf: expectedWebp))
    let newSession = try await loaded()
    #expect(newSession.saveSettings == SaveSettings())
  }
  @Test func actualSizeUsesBackingPixels() {
    let canvas = CanvasView(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
    canvas.imageSize = CGSize(width: 1000, height: 660)
    canvas.actualSize()
    #expect(canvas.transform.scale == 0.5)
    #expect(canvas.zoomPercent == 100)
  }
}

@Test func compactWindowsShowTheImageAtActualSizeWithinTheScreen() {
  let visible = CGRect(x: 0, y: 30, width: 1600, height: 970)
  func frame(_ width: CGFloat, _ height: CGFloat, around: CGRect) -> CGRect {
    EditorWindow.compactFrame(
      image: CGSize(width: width, height: height), scale: 2,
      decor: CGSize(width: 0, height: 140), around: around,
      minimum: CGSize(width: 560, height: 320), visible: visible)
  }
  let centered = CGRect(x: 240, y: 100, width: 1120, height: 780)
  // 2000×1400 px is 1000×700 pt at 2×; the canvas adds only the Fit margin.
  #expect(frame(2000, 1400, around: centered) == CGRect(x: 280, y: 50, width: 1040, height: 880))
  // Too tall for the screen: both axes shrink together, so no side bars appear.
  #expect(frame(4032, 3024, around: centered) == CGRect(x: 253, y: 30, width: 1094, height: 970))
  #expect(frame(200, 100, around: centered) == CGRect(x: 520, y: 330, width: 560, height: 320))
  let corner = CGRect(x: 1500, y: 900, width: 1120, height: 780)
  #expect(visible.contains(frame(2000, 1400, around: corner)))
}

@Test func exportFormatsChooseVisibleExtensions() {
  #expect(ExportFormat.forFilename("photo.JPEG") == .jpeg)
  #expect(ExportFormat.forFilename("photo.HEIF") == .heic)
  #expect(ExportFormat.forFilename("scan.tiff") == .tiff)
  #expect(ExportFormat.forFilename("photo.psd") == .png)
  #expect(ExportFormat.webp.filename("한글 photo.png") == "한글 photo.webp")
  #expect(ExportFormat.jpeg.filename("photo") == "photo.jpg")
  #expect(ExportFormat.jpeg2000.filename("photo.png") == "photo.jp2")
  #expect(ExportFormat.available.first == .png)
  #expect(ExportFormat.available.contains(.heic))
  // AVIF is the only format whose encoder depends on the macOS version.
  #expect(
    ExportFormat.allCases.filter { !ExportFormat.available.contains($0) }.allSatisfy { $0 == .avif }
  )
  #expect(Set(ExportFormat.allCases.map(\.type)).count == ExportFormat.allCases.count)
}

extension SessionTests {
  @Test func quittingFreezesEditingAndRestoresItOnCancellation() async throws {
    _ = NSApplication.shared
    let editor = EditorWindow()
    defer { editor.session.invalidate() }
    editor.session.load(fixture)
    try await settled(editor.session)
    editor.session.apply(.resize(200, 132))
    try await settled(editor.session)
    let revision = editor.session.generation
    editor.interactionBlocked = true
    #expect(!editor.canvas.enabled)
    editor.undo(nil)
    #expect(editor.session.generation == revision)
    #expect(!editor.windowShouldClose(editor.window!))
    editor.interactionBlocked = false
    #expect(editor.canvas.enabled)
    editor.undo(nil)
    try await settled(editor.session)
    #expect(editor.session.state?.width == 1000)
  }
  @Test func firstImageSizesTheWindowAndLaterEditsKeepIt() async throws {
    _ = NSApplication.shared
    let editor = EditorWindow()
    defer { editor.session.invalidate() }
    let window = try #require(editor.window)
    let initial = window.frame
    editor.session.load(fixture)
    try await settled(editor.session)
    let fitted = window.frame
    #expect(fitted != initial)
    window.contentView?.layoutSubtreeIfNeeded()
    // A display too small for the fixture fills one axis and shrinks the image instead.
    let visible = try #require((window.screen ?? NSScreen.main)?.visibleFrame)
    #expect(
      editor.canvas.zoomPercent == 100 || fitted.width == visible.width
        || fitted.height == visible.height)
    editor.session.apply(.resize(200, 132))
    try await settled(editor.session)
    #expect(window.frame == fitted)
  }
  @Test func openingShowsTheWindowOnlyAfterTheImageLoaded() async throws {
    _ = NSApplication.shared
    let editor = EditorWindow()
    defer { editor.session.invalidate() }
    let window = try #require(editor.window)
    editor.open(fixture)
    #expect(editor.session.busy && !window.isVisible)
    try await settled(editor.session)
    #expect(window.isVisible && editor.session.state?.width == 1000)
    window.orderOut(nil)
    let failing = EditorWindow()
    defer { failing.session.invalidate() }
    let missing = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID()).png")
    failing.open(missing)
    try await settled(failing.session)
    #expect(failing.window?.isVisible == true && failing.session.error != nil)
    failing.window?.orderOut(nil)
  }
  @Test func detectingProtectsRegionsButAllowsOtherTools() async throws {
    _ = NSApplication.shared
    let editor = EditorWindow()
    editor.session.load(fixture)
    try await settled(editor.session)
    editor.selectTool(.text)
    editor.session.detect()
    #expect(!editor.canvas.enabled)
    editor.selectTool(.highlight)
    #expect(editor.canvas.enabled)
    editor.selectTool(.text)
    #expect(!editor.canvas.enabled)
    // Invalidate before the queued snapshot returns, so this state test does not
    // start a Vision request. Native OCR is covered by the Rust integration tests.
    editor.session.invalidate()
  }
}
