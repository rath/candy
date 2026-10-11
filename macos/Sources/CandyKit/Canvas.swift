import AppKit

enum Tool: Int, CaseIterable {
  case select, mosaic, highlight, text, resize, flatten
  var title: String {
    ["Select", "Mosaic", "Highlight", "Text mosaic", "Resize", "Flatten alpha"][rawValue]
  }
  var symbol: String {
    [
      "selection.pin.in.out", "square.grid.3x3", "highlighter", "text.viewfinder",
      "arrow.up.left.and.arrow.down.right", "square.on.circle",
    ][rawValue]
  }
}
struct CanvasTransform {
  var origin: CGPoint, scale: CGFloat
  func image(_ point: CGPoint) -> CGPoint {
    CGPoint(x: (point.x - origin.x) / scale, y: (point.y - origin.y) / scale)
  }
  func screen(_ point: CGPoint) -> CGPoint {
    CGPoint(x: origin.x + point.x * scale, y: origin.y + point.y * scale)
  }
  func rect(_ r: PixelRect) -> CGRect {
    CGRect(
      origin: screen(r.cgRect.origin),
      size: CGSize(width: CGFloat(r.width) * scale, height: CGFloat(r.height) * scale))
  }
}
struct Regions: Equatable {
  var selection: PixelRect?
  var boxes: [PixelRect] = []
  var active: Int?
  mutating func removeActive() {
    if let active, boxes.indices.contains(active) { boxes.remove(at: active) }
    active = nil
  }
  func selected(_ tool: Tool) -> PixelRect? {
    tool == .text ? active.flatMap { boxes.indices.contains($0) ? boxes[$0] : nil } : selection
  }
  mutating func set(_ rect: PixelRect?, tool: Tool) {
    if tool == .text {
      guard let active, boxes.indices.contains(active) else { return }
      if let rect { boxes[active] = rect } else { removeActive() }
    } else {
      selection = rect
    }
  }
}

@MainActor
final class CanvasView: NSView {
  override var isFlipped: Bool { true }
  override var acceptsFirstResponder: Bool { true }
  override func setFrameSize(_ newSize: NSSize) {
    super.setFrameSize(newSize)
    needsDisplay = true
    onChange?()
  }
  override func viewDidMoveToWindow() {
    super.viewDidMoveToWindow()
    onChange?()
  }
  var image: CGImage? { didSet { needsDisplay = true } }
  var imageSize = CGSize.zero
  var tool: Tool = .select {
    didSet {
      cancel()
      window?.invalidateCursorRects(for: self)
    }
  }
  var enabled = true
  var regions = Regions() {
    didSet {
      needsDisplay = true
      onChange?()
    }
  }
  var brushColor = NSColor.systemYellow.withAlphaComponent(0.4)
  var brushWidth: CGFloat = 28
  var zoom: CGFloat?
  var pan = CGPoint.zero
  var onChange: (() -> Void)?
  var onStroke: (([CGPoint]) -> Void)?
  private var prior = Regions()
  private var space = false
  private var tracking: NSTrackingArea?
  private enum Drag {
    case new(CGPoint)
    case move(CGPoint, PixelRect)
    case resize(CGPoint)
    case pan(CGPoint, CGPoint)
    case brush([CGPoint])
  }
  private var drag: Drag?
  private var accent: NSColor {
    effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
      ? NSColor(srgbRed: 1, green: 0.608, blue: 0.514, alpha: 1)
      : NSColor(srgbRed: 0.718, green: 0.275, blue: 0.196, alpha: 1)
  }
  /// Total space Fit leaves around the image on each axis.
  nonisolated static let fitMargin: CGFloat = 40
  var transform: CanvasTransform {
    let backing = window?.backingScaleFactor ?? 2
    let margin = CanvasView.fitMargin
    let fit = min(
      max(1, bounds.width - margin) / max(1, imageSize.width),
      max(1, bounds.height - margin) / max(1, imageSize.height), 1 / backing)
    let scale = zoom.map { $0 / backing } ?? fit
    return CanvasTransform(
      origin: CGPoint(
        x: (bounds.width - imageSize.width * scale) / 2 + pan.x,
        y: (bounds.height - imageSize.height * scale) / 2 + pan.y), scale: scale)
  }
  var zoomPercent: Int {
    Int((transform.scale * (window?.backingScaleFactor ?? 2) * 100).rounded())
  }
  func fit() {
    zoom = nil
    pan = .zero
    needsDisplay = true
    onChange?()
  }
  func actualSize() {
    zoom = 1
    pan = .zero
    needsDisplay = true
    onChange?()
  }
  func clear() {
    drag = nil
    regions = Regions()
  }
  func cancel() {
    if drag != nil { regions = prior }
    drag = nil
    needsDisplay = true
  }
  override func resetCursorRects() { addCursorRect(bounds, cursor: space ? .openHand : .crosshair) }
  override func updateTrackingAreas() {
    if let tracking { removeTrackingArea(tracking) }
    tracking = NSTrackingArea(
      rect: .zero, options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved], owner: self)
    addTrackingArea(tracking!)
    super.updateTrackingAreas()
  }
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    needsDisplay = true
  }
  override func draw(_ dirtyRect: NSRect) {
    NSColor.underPageBackgroundColor.setFill()
    bounds.fill()
    guard let image, imageSize.width > 0 else { return }
    let t = transform
    let imageRect = CGRect(
      origin: t.origin,
      size: CGSize(width: imageSize.width * t.scale, height: imageSize.height * t.scale))
    let visible = bounds.intersection(imageRect)
    guard !visible.isNull else { return }
    NSGraphicsContext.saveGraphicsState()
    visible.clip()
    let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    for y in stride(from: visible.minY, to: visible.maxY, by: 12) {
      for x in stride(from: visible.minX, to: visible.maxX, by: 12) {
        let alternate = (Int((x - visible.minX) / 12) + Int((y - visible.minY) / 12)) % 2 == 0
        NSColor(white: dark ? (alternate ? 0.21 : 0.16) : (alternate ? 0.96 : 0.91), alpha: 1)
          .setFill()
        CGRect(x: x, y: y, width: 12, height: 12).fill()
      }
    }
    if let context = NSGraphicsContext.current?.cgContext {
      context.saveGState()
      context.translateBy(x: imageRect.minX, y: imageRect.maxY)
      context.scaleBy(x: 1, y: -1)
      context.interpolationQuality = .high
      context.draw(image, in: CGRect(origin: .zero, size: imageRect.size))
      context.restoreGState()
    }
    if case .brush(let points) = drag, let first = points.first {
      brushColor.setStroke()
      let path = NSBezierPath()
      path.lineWidth = brushWidth * t.scale
      path.lineCapStyle = .round
      path.lineJoinStyle = .round
      path.move(to: t.screen(first))
      for point in points.dropFirst() { path.line(to: t.screen(point)) }
      if points.count == 1 { path.line(to: t.screen(CGPoint(x: first.x + 0.01, y: first.y))) }
      path.stroke()
    }
    if tool == .text {
      for (index, box) in regions.boxes.enumerated() {
        drawRegion(t.rect(box), active: regions.active == index, fill: true)
      }
    } else if let selection = regions.selection {
      drawRegion(t.rect(selection), active: true, fill: false)
    }
    NSGraphicsContext.restoreGraphicsState()
  }
  private func drawRegion(_ rect: CGRect, active: Bool, fill: Bool) {
    if fill {
      accent.withAlphaComponent(0.12).setFill()
      rect.fill()
    }
    accent.setStroke()
    let path = NSBezierPath(rect: rect)
    path.lineWidth = active ? 2 : 1
    path.stroke()
    if active {
      for point in corners(rect) {
        let handle = CGRect(x: point.x - 3.5, y: point.y - 3.5, width: 7, height: 7)
        NSColor.white.setFill()
        handle.fill()
        accent.setStroke()
        NSBezierPath(rect: handle).stroke()
      }
    }
  }
  private func corners(_ r: CGRect) -> [CGPoint] {
    [
      CGPoint(x: r.minX, y: r.minY), CGPoint(x: r.maxX, y: r.minY), CGPoint(x: r.maxX, y: r.maxY),
      CGPoint(x: r.minX, y: r.maxY),
    ]
  }
  override func mouseDown(with event: NSEvent) {
    guard enabled, image != nil else { return }
    window?.makeFirstResponder(self)
    let screen = convert(event.locationInWindow, from: nil)
    let p = transform.image(screen)
    prior = regions
    if space {
      drag = .pan(screen, pan)
      return
    }
    if tool == .highlight, CGRect(origin: .zero, size: imageSize).contains(p) {
      drag = .brush([p])
      needsDisplay = true
      return
    }
    guard [.select, .mosaic, .text].contains(tool) else { return }
    if tool == .text {
      // Test the current region first so overlapping boxes do not steal its handles.
      let active = regions.active.flatMap { regions.boxes.indices.contains($0) ? $0 : nil }
      regions.active =
        active.flatMap {
          transform.rect(regions.boxes[$0]).insetBy(dx: -9, dy: -9).contains(screen) ? $0 : nil
        }
        ?? regions.boxes.indices.reversed().first {
          transform.rect(regions.boxes[$0]).insetBy(dx: -7, dy: -7).contains(screen)
        }
    }
    if let rect = regions.selected(tool) {
      let c = corners(transform.rect(rect))
      if let index = c.firstIndex(where: { hypot($0.x - screen.x, $0.y - screen.y) < 9 }) {
        drag = .resize(transform.image(c[(index + 2) % 4]))
        return
      }
      if transform.rect(rect).contains(screen) {
        drag = .move(p, rect)
        return
      }
    }
    if CGRect(origin: .zero, size: imageSize).contains(p) {
      if tool == .text {
        regions.boxes.append(PixelRect(x: UInt32(p.x), y: UInt32(p.y), width: 1, height: 1))
        regions.active = regions.boxes.count - 1
      }
      drag = .new(p)
    }
  }
  override func mouseDragged(with event: NSEvent) {
    let screen = convert(event.locationInWindow, from: nil)
    let p = transform.image(screen)
    switch drag {
    case .new(let start), .resize(let start):
      if let rect = PixelRect.between(start, p, size: imageSize) { regions.set(rect, tool: tool) }
    case .move(let start, let rect):
      var moved = rect
      moved.x = UInt32(
        min(max(0, CGFloat(rect.x) + p.x - start.x), imageSize.width - CGFloat(rect.width))
          .rounded())
      moved.y = UInt32(
        min(max(0, CGFloat(rect.y) + p.y - start.y), imageSize.height - CGFloat(rect.height))
          .rounded())
      regions.set(moved, tool: tool)
    case .pan(let start, let original):
      pan = CGPoint(x: original.x + screen.x - start.x, y: original.y + screen.y - start.y)
    case .brush(var points):
      points.append(p)
      drag = .brush(points)
    case nil: break
    }
    needsDisplay = true
    onChange?()
  }
  override func mouseUp(with event: NSEvent) {
    if case .brush(let points) = drag { onStroke?(points) }
    drag = nil
    needsDisplay = true
  }
  override func keyDown(with event: NSEvent) {
    if event.keyCode == 53 {
      if drag != nil {
        cancel()
      } else {
        regions.selection = nil
        regions.active = nil
      }
      return
    }
    if event.keyCode == 49 {
      space = true
      window?.invalidateCursorRects(for: self)
      return
    }
    if enabled, tool == .text, [51, 117].contains(event.keyCode) {
      regions.removeActive()
      return
    }
    super.keyDown(with: event)
  }
  override func keyUp(with event: NSEvent) {
    if event.keyCode == 49 {
      space = false
      window?.invalidateCursorRects(for: self)
    } else {
      super.keyUp(with: event)
    }
  }
  override func resignFirstResponder() -> Bool {
    space = false
    cancel()
    return super.resignFirstResponder()
  }
  private func magnify(_ factor: CGFloat, at screen: CGPoint) {
    guard image != nil else { return }
    let before = transform.image(screen)
    zoom = min(16, max(0.01, (zoom ?? CGFloat(zoomPercent) / 100) * factor))
    let after = transform.screen(before)
    pan.x += screen.x - after.x
    pan.y += screen.y - after.y
    needsDisplay = true
    onChange?()
  }
  override func scrollWheel(with event: NSEvent) {
    magnify(exp(event.scrollingDeltaY * 0.01), at: convert(event.locationInWindow, from: nil))
  }
  override func magnify(with event: NSEvent) {
    magnify(max(0.1, 1 + event.magnification), at: convert(event.locationInWindow, from: nil))
  }
}
