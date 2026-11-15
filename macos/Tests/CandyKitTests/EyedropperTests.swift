import AppKit
import ImageIO
import Testing
import UniformTypeIdentifiers

@testable import CandyKit

private var fixture: URL {
  URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(
      "crates/candy-macos/tests/fixtures/sample.png")
}

/// Writes an opaque sRGB PNG whose pixel at (x, y) is `color(x, y)`, and returns its folder.
private func makeImage(width: Int, height: Int, color: (Int, Int) -> PixelColor) throws -> URL {
  var bytes = [UInt8]()
  bytes.reserveCapacity(width * height * 4)
  for y in 0..<height {
    for x in 0..<width {
      let c = color(x, y)
      bytes += [c.red, c.green, c.blue, 255]
    }
  }
  let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
  let provider = try #require(CGDataProvider(data: Data(bytes) as CFData))
  let image = try #require(
    CGImage(
      width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
      bytesPerRow: width * 4, space: space,
      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
      provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent))
  let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
  let destination = try #require(
    CGImageDestinationCreateWithURL(
      folder.appendingPathComponent("colors.png") as CFURL, UTType.png.identifier as CFString, 1,
      nil))
  CGImageDestinationAddImage(destination, image, nil)
  #expect(CGImageDestinationFinalize(destination))
  return folder
}
/// A color that identifies its own position, for images up to 65536 pixels wide.
private func positional(_ x: Int, _ y: Int) -> PixelColor {
  PixelColor(red: UInt8(x % 256), green: UInt8(x / 256), blue: UInt8(y), alpha: 255)
}

@Test func pixelColorsFormatAsHexAndDecimal() {
  let opaque = PixelColor(red: 250, green: 172, blue: 48, alpha: 255)
  #expect(opaque.hex == "#faac30" && opaque.decimal == "250, 172, 48")
  let translucent = PixelColor(red: 0, green: 9, blue: 255, alpha: 128)
  #expect(translucent.hex == "#0009ff80" && translucent.decimal == "0, 9, 255, 128")
  let srgb = opaque.color.usingColorSpace(.sRGB)
  #expect(srgb?.redComponent == 250.0 / 255 && srgb?.alphaComponent == 1)
}

@Test func loupeStaysBesideItsTargetAndInsideTheCanvas() {
  let bounds = CGRect(x: 0, y: 0, width: 600, height: 400)
  let size = LoupeView.size
  let gap = LoupeView.gap
  #expect(
    LoupeView.origin(near: CGPoint(x: 100, y: 80), in: bounds)
      == CGPoint(x: 100 + gap, y: 80 + gap))
  // Near the right and bottom edges it moves to the other side of the target.
  #expect(
    LoupeView.origin(near: CGPoint(x: 590, y: 390), in: bounds)
      == CGPoint(x: 590 - gap - size.width, y: 390 - gap - size.height))
  #expect(
    LoupeView.origin(near: CGPoint(x: 590, y: 80), in: bounds)
      == CGPoint(x: 590 - gap - size.width, y: 80 + gap))
  for anchor in [CGPoint(x: -500, y: 900), CGPoint(x: 5, y: 5), CGPoint(x: 599, y: 0)] {
    let frame = CGRect(origin: LoupeView.origin(near: anchor, in: bounds), size: size)
    #expect(bounds.contains(frame))
  }
}

@Test func neighborhoodsReadFullResolutionPixelsAndStopAtTheImageEdge() throws {
  // Wider than the 4096 px preview limit: preview pixels would be averages of neighbors.
  let folder = try makeImage(width: 5000, height: 3, color: positional)
  defer { try? FileManager.default.removeItem(at: folder) }
  let rendered = try RustDocument.open(folder.appendingPathComponent("colors.png")).render()
  #expect(rendered.preview.width == 4096)
  let pixels = rendered.pixels
  #expect(pixels.width == 5000 && pixels.height == 3)
  #expect(pixels.color(at: PixelPoint(x: 4999, y: 2)) == positional(4999, 2))
  #expect(pixels.color(at: PixelPoint(x: 5000, y: 2)) == nil)
  #expect(pixels.color(at: PixelPoint(x: 0, y: -1)) == nil)
  let around = pixels.neighborhood(of: PixelPoint(x: 4998, y: 0), radius: 2)
  #expect(around.count == 25)
  for row in 0..<5 {
    for column in 0..<5 {
      let x = 4996 + column
      let y = row - 2
      let inside = x < 5000 && (0..<3).contains(y)
      #expect(around[row * 5 + column] == (inside ? positional(x, y) : nil))
    }
  }
  #expect(pixels.neighborhood(of: PixelPoint(x: 9000, y: 9000), radius: 1) == [nil] + Array(
    repeating: nil, count: 8))
}

extension SessionTests {
  private func finished(_ session: DocumentSession) async throws {
    for _ in 0..<500 {
      if !session.busy { return }
      try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("Document operation did not finish")
  }
  private func mouse(_ type: NSEvent.EventType, _ canvas: CanvasView, at pixel: CGPoint) throws {
    let point = canvas.convert(canvas.transform.screen(pixel), to: nil)
    let event = try #require(
      NSEvent.mouseEvent(
        with: type, location: point, modifierFlags: [], timestamp: 0,
        windowNumber: canvas.window?.windowNumber ?? 0, context: nil, eventNumber: 0,
        clickCount: 1, pressure: 1))
    switch type {
    case .leftMouseDown: canvas.mouseDown(with: event)
    case .leftMouseDragged: canvas.mouseDragged(with: event)
    case .leftMouseUp: canvas.mouseUp(with: event)
    default: canvas.mouseMoved(with: event)
    }
  }
  private func key(_ code: UInt16, _ canvas: CanvasView, shift: Bool = false) throws {
    let event = try #require(
      NSEvent.keyEvent(
        with: .keyDown, location: .zero, modifierFlags: shift ? .shift : [], timestamp: 0,
        windowNumber: canvas.window?.windowNumber ?? 0, context: nil, characters: "",
        charactersIgnoringModifiers: "", isARepeat: false, keyCode: code))
    canvas.keyDown(with: event)
  }
  private func eyedropperCanvas() throws -> (CanvasView, LoupeView, URL) {
    let folder = try makeImage(width: 300, height: 200, color: positional)
    let rendered = try RustDocument.open(folder.appendingPathComponent("colors.png")).render()
    let canvas = CanvasView(frame: CGRect(x: 0, y: 0, width: 600, height: 400))
    canvas.image = rendered.preview.image()
    canvas.imageSize = rendered.state.size
    canvas.pixels = rendered.pixels
    canvas.tool = .eyedropper
    let loupe = try #require(canvas.subviews.compactMap { $0 as? LoupeView }.first)
    return (canvas, loupe, folder)
  }

  @Test func eyedropperMagnifiesThePointerAndPinsTheClickedPixel() throws {
    _ = NSApplication.shared
    let (canvas, loupe, folder) = try eyedropperCanvas()
    defer { try? FileManager.default.removeItem(at: folder) }
    #expect(loupe.isHidden && canvas.sample == nil)
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 120.5, y: 80.5))
    #expect(!loupe.isHidden && canvas.bounds.contains(loupe.frame))
    #expect(loupe.pixels[loupe.pixels.count / 2] == positional(120, 80))
    #expect(loupe.pixels[0] == positional(115, 75))
    #expect(canvas.pin == nil)
    #expect(
      canvas.sample
        == ColorSample(point: PixelPoint(x: 120, y: 80), color: positional(120, 80)))
    // The first and last pixels have neighbors outside the image.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 0.5, y: 199.5))
    #expect(loupe.pixels[loupe.pixels.count / 2] == positional(0, 199))
    #expect(loupe.pixels[0] == nil && loupe.pixels[loupe.pixels.count - 1] == nil)
    // Beside the image there is nothing to magnify.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: -3, y: 80))
    #expect(loupe.isHidden && canvas.sample == nil)
    try mouse(.leftMouseDown, canvas, at: CGPoint(x: -3, y: 80))
    #expect(canvas.pin == nil)

    try mouse(.leftMouseDown, canvas, at: CGPoint(x: 40.2, y: 30.9))
    #expect(canvas.pin == PixelPoint(x: 40, y: 30))
    try mouse(.leftMouseDragged, canvas, at: CGPoint(x: 41.5, y: 33.5))
    try mouse(.leftMouseUp, canvas, at: CGPoint(x: 41.5, y: 33.5))
    #expect(canvas.pin == PixelPoint(x: 41, y: 33))
    // The sample stays on the pin while the loupe keeps following the pointer.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 200.5, y: 100.5))
    #expect(canvas.sample?.color == positional(41, 33))
    #expect(loupe.pixels[loupe.pixels.count / 2] == positional(200, 100))
    canvas.forgetPointer()
    #expect(loupe.isHidden && canvas.sample?.color == positional(41, 33))

    // A locked document keeps its pin, without a loupe or new pins.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 200.5, y: 100.5))
    canvas.enabled = false
    #expect(loupe.isHidden)
    try mouse(.leftMouseDown, canvas, at: CGPoint(x: 10.5, y: 10.5))
    #expect(canvas.pin == PixelPoint(x: 41, y: 33))
    canvas.enabled = true
    #expect(!loupe.isHidden)
    // Other tools do not show the loupe, and the pin is waiting on return.
    canvas.tool = .select
    #expect(loupe.isHidden)
    canvas.tool = .eyedropper
    #expect(!loupe.isHidden && canvas.pin == PixelPoint(x: 41, y: 33))
  }

  @Test func arrowKeysMoveThePinByPixelsAndEscapeClearsIt() throws {
    _ = NSApplication.shared
    let (canvas, loupe, folder) = try eyedropperCanvas()
    defer { try? FileManager.default.removeItem(at: folder) }
    let selection = PixelRect(x: 5, y: 5, width: 20, height: 20)
    canvas.regions.selection = selection
    // Without a pin or a pointer there is nothing to move.
    try key(124, canvas)
    #expect(canvas.pin == nil)
    // The first arrow key pins the hovered pixel, moved by one.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 150.5, y: 100.5))
    try key(124, canvas)
    #expect(canvas.pin == PixelPoint(x: 151, y: 100))
    try key(125, canvas)
    try key(123, canvas)
    try key(123, canvas)
    #expect(canvas.pin == PixelPoint(x: 149, y: 101))
    try key(126, canvas, shift: true)
    #expect(canvas.pin == PixelPoint(x: 149, y: 91))
    // The loupe turns to the pin, next to where the pin is drawn.
    #expect(loupe.pixels[loupe.pixels.count / 2] == positional(149, 91))
    #expect(canvas.sample?.color == positional(149, 91))
    let pinCenter = canvas.transform.screen(CGPoint(x: 149.5, y: 91.5))
    #expect(loupe.frame.origin == LoupeView.origin(near: pinCenter, in: canvas.bounds))
    // Moving the pointer takes the loupe back.
    try mouse(.mouseMoved, canvas, at: CGPoint(x: 10.5, y: 10.5))
    #expect(loupe.pixels[loupe.pixels.count / 2] == positional(10, 10))
    // The pin stops at the image edges.
    for _ in 0..<40 { try key(124, canvas, shift: true) }
    for _ in 0..<40 { try key(125, canvas, shift: true) }
    #expect(canvas.pin == PixelPoint(x: 299, y: 199))
    for _ in 0..<40 { try key(123, canvas, shift: true) }
    for _ in 0..<40 { try key(126, canvas, shift: true) }
    #expect(canvas.pin == PixelPoint(x: 0, y: 0))

    // Escape cancels a pinning drag, then clears the pin, then the selection.
    try mouse(.leftMouseDown, canvas, at: CGPoint(x: 60.5, y: 60.5))
    #expect(canvas.pin == PixelPoint(x: 60, y: 60))
    try key(53, canvas)
    #expect(canvas.pin == PixelPoint(x: 0, y: 0))
    try mouse(.leftMouseUp, canvas, at: CGPoint(x: 60.5, y: 60.5))
    try key(53, canvas)
    #expect(canvas.pin == nil && canvas.regions.selection == selection)
    try key(53, canvas)
    #expect(canvas.regions.selection == nil)
  }

  @Test func eyedropperStripShowsThePinnedColorAndFollowsTheDocument() async throws {
    _ = NSApplication.shared
    let editor = EditorWindow()
    defer { editor.session.invalidate() }
    editor.session.load(fixture)
    try await finished(editor.session)
    let content = try #require(editor.window?.contentView)
    content.layoutSubtreeIfNeeded()
    func controls<T: NSView>(_: T.Type, in view: NSView) -> [T] {
      (view as? T).map { [$0] } ?? view.subviews.flatMap { controls(T.self, in: $0) }
    }
    // The strip is rebuilt whenever a tool is chosen, so look its controls up each time.
    // Each value is the title of the button that copies it.
    func copyButton(_ title: String) throws -> NSButton {
      try #require(
        controls(NSButton.self, in: content).first { $0.accessibilityLabel() == title })
    }
    func values() throws -> [String] {
      try [localized("Copy hex"), localized("Copy RGB")].map { try copyButton($0).title }
    }
    func enabled() throws -> [Bool] {
      try [localized("Copy hex"), localized("Copy RGB")].map { try copyButton($0).isEnabled }
    }
    func swatch() throws -> PixelColor? {
      try #require(controls(SwatchView.self, in: content).first).color
    }
    let copy = NSMenuItem(
      title: "Copy", action: #selector(EditorWindow.copy(_:)), keyEquivalent: "c")
    editor.selectTool(.eyedropper)
    #expect(editor.window?.firstResponder === editor.canvas)
    #expect(try values() == ["–", "–"] && enabled() == [false, false])
    #expect(try swatch() == nil)
    #expect(!editor.validateMenuItem(copy))

    let pixel = PixelPoint(x: 321, y: 123)
    let expected = try #require(try RustDocument.open(fixture).render().pixels.color(at: pixel))
    try mouse(.leftMouseDown, editor.canvas, at: CGPoint(x: 321.5, y: 123.5))
    try mouse(.leftMouseUp, editor.canvas, at: CGPoint(x: 321.5, y: 123.5))
    #expect(editor.canvas.pin == pixel)
    #expect(try values() == [expected.hex, expected.decimal] && enabled() == [true, true])
    #expect(try swatch() == expected)
    #expect(editor.validateMenuItem(copy))
    let spoken = editor.canvas.accessibilityValue() as? String
    #expect(spoken?.contains("321, 123 · \(expected.hex)") == true)
    // Copy belongs to the eyedropper; other tools leave it to text fields. The pin waits.
    editor.selectTool(.select)
    #expect(!editor.validateMenuItem(copy))
    editor.selectTool(.eyedropper)
    #expect(editor.validateMenuItem(copy))
    #expect(try values() == [expected.hex, expected.decimal])

    // An edit replaces the pixels, so the pin goes with the old revision and the strip
    // returns to the pixel under the pointer, read from the new revision.
    editor.session.apply(.rotate(clockwise: true))
    try await finished(editor.session)
    let hovered = try #require(editor.canvas.sample)
    #expect(editor.canvas.pin == nil && hovered.point != pixel)
    #expect(editor.canvas.pixels?.color(at: hovered.point) == hovered.color)
    #expect(try values() == [hovered.color.hex, hovered.color.decimal])
    editor.canvas.forgetPointer()
    #expect(editor.canvas.sample == nil)
    #expect(try values() == ["–", "–"] && enabled() == [false, false] && swatch() == nil)
    #expect(editor.canvas.pixels?.width == 660 && editor.canvas.pixels?.height == 1000)
  }
}
