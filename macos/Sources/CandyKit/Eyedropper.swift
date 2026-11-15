import AppKit

/// A pixel position in the oriented, full-resolution image.
struct PixelPoint: Equatable, Sendable {
  var x: Int, y: Int
}
/// One pixel as the document stores and exports it: straight-alpha 8-bit sRGB.
struct PixelColor: Equatable, Sendable {
  var red: UInt8, green: UInt8, blue: UInt8, alpha: UInt8
  private var channels: [UInt8] { alpha == 255 ? [red, green, blue] : [red, green, blue, alpha] }
  /// `#rrggbb`, or `#rrggbbaa` when the pixel is not opaque.
  var hex: String { "#" + channels.map { String(format: "%02x", $0) }.joined() }
  /// `r, g, b` from 0–255, followed by alpha when the pixel is not opaque.
  var decimal: String { channels.map { String($0) }.joined(separator: ", ") }
  var color: NSColor {
    NSColor(
      srgbRed: CGFloat(red) / 255, green: CGFloat(green) / 255, blue: CGFloat(blue) / 255,
      alpha: CGFloat(alpha) / 255)
  }
}
struct ColorSample: Equatable {
  var point: PixelPoint
  var color: PixelColor
}

/// Fills `rect` with the transparency checker, in cells of `cell` points, then with `color`.
@MainActor
private func fill(_ rect: CGRect, with color: PixelColor, cell: CGFloat, dark: Bool) {
  if color.alpha < 255 {
    for row in 0..<Int((rect.height / cell).rounded(.up)) {
      for column in 0..<Int((rect.width / cell).rounded(.up)) {
        let alternate = (row + column) % 2 == 0
        NSColor(white: dark ? (alternate ? 0.21 : 0.16) : (alternate ? 0.96 : 0.91), alpha: 1)
          .setFill()
        CGRect(
          x: rect.minX + CGFloat(column) * cell, y: rect.minY + CGFloat(row) * cell, width: cell,
          height: cell
        ).intersection(rect).fill()
      }
    }
  }
  color.color.setFill()
  rect.fill(using: .sourceOver)
}

enum PixelMarker {
  /// Outlines `rect` from outside, leaving the marked pixel uncovered: a 2 pt accent line
  /// inside a 1 pt white one, so it shows on any image color.
  @MainActor static func draw(around rect: CGRect, accent: NSColor) {
    NSColor.white.setStroke()
    let outer = NSBezierPath(rect: rect.insetBy(dx: -2.5, dy: -2.5))
    outer.lineWidth = 1
    outer.stroke()
    accent.setStroke()
    let inner = NSBezierPath(rect: rect.insetBy(dx: -1, dy: -1))
    inner.lineWidth = 2
    inner.stroke()
  }
}

/// The magnified pixels around the eyedropper's target, with the target's color values.
/// It is an overlay of the canvas, which positions it and passes the mouse through.
@MainActor
final class LoupeView: NSView {
  /// Pixels shown on each side of the target.
  nonisolated static let radius = 5
  nonisolated static let cell: CGFloat = 12
  nonisolated static let border: CGFloat = 2
  nonisolated static let caption: CGFloat = 40
  /// Distance from the anchor, so the loupe does not cover what it magnifies.
  nonisolated static let gap: CGFloat = 18
  nonisolated static var size: CGSize {
    let grid = CGFloat(2 * radius + 1) * cell
    return CGSize(width: grid + 2 * border, height: grid + caption + 2 * border)
  }
  /// Where the loupe goes for a target at `anchor`: below and to the right, on the opposite
  /// side where that would leave `bounds`, and in any case inside `bounds`.
  nonisolated static func origin(near anchor: CGPoint, in bounds: CGRect) -> CGPoint {
    var x = anchor.x + gap
    var y = anchor.y + gap
    if x + size.width > bounds.maxX { x = anchor.x - gap - size.width }
    if y + size.height > bounds.maxY { y = anchor.y - gap - size.height }
    return CGPoint(
      x: max(bounds.minX, min(x, bounds.maxX - size.width)).rounded(),
      y: max(bounds.minY, min(y, bounds.maxY - size.height)).rounded())
  }

  override var isFlipped: Bool { true }
  /// Row by row, (2 × radius + 1)² entries; nil outside the image.
  var pixels: [PixelColor?] = [] {
    didSet { if pixels != oldValue { needsDisplay = true } }
  }
  var accent = NSColor.controlAccentColor {
    didSet { if accent != oldValue { needsDisplay = true } }
  }
  override func hitTest(_ point: NSPoint) -> NSView? { nil }
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    needsDisplay = true
  }
  override func draw(_ dirtyRect: NSRect) {
    let side = 2 * LoupeView.radius + 1
    guard pixels.count == side * side else { return }
    let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    let cell = LoupeView.cell
    // A dark line around a white one separates the loupe from any image behind it.
    NSColor.white.setFill()
    bounds.fill()
    NSColor.black.withAlphaComponent(0.6).setFill()
    bounds.frame(withWidth: 1, using: .sourceOver)
    let grid = CGRect(
      x: LoupeView.border, y: LoupeView.border, width: CGFloat(side) * cell,
      height: CGFloat(side) * cell)
    for row in 0..<side {
      for column in 0..<side {
        let rect = CGRect(
          x: grid.minX + CGFloat(column) * cell, y: grid.minY + CGFloat(row) * cell, width: cell,
          height: cell)
        if let pixel = pixels[row * side + column] {
          fill(rect, with: pixel, cell: cell / 2, dark: dark)
        } else {
          NSColor.underPageBackgroundColor.setFill()
          rect.fill()
        }
      }
    }
    NSColor.black.withAlphaComponent(0.12).setFill()
    for line in 1..<side {
      let offset = CGFloat(line) * cell - 0.25
      CGRect(x: grid.minX + offset, y: grid.minY, width: 0.5, height: grid.height)
        .fill(using: .sourceOver)
      CGRect(x: grid.minX, y: grid.minY + offset, width: grid.width, height: 0.5)
        .fill(using: .sourceOver)
    }
    let middle = CGFloat(LoupeView.radius) * cell
    PixelMarker.draw(
      around: CGRect(x: grid.minX + middle, y: grid.minY + middle, width: cell, height: cell),
      accent: accent)
    let caption = CGRect(
      x: grid.minX, y: grid.maxY, width: grid.width, height: LoupeView.caption)
    NSColor.windowBackgroundColor.setFill()
    caption.fill()
    NSColor.separatorColor.setFill()
    CGRect(x: caption.minX, y: caption.minY, width: caption.width, height: 1)
      .fill(using: .sourceOver)
    guard let target = pixels[pixels.count / 2] else { return }
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    let font = NSFont.monospacedDigitSystemFont(
      ofSize: NSFont.smallSystemFontSize, weight: .regular)
    for (index, (text, color)) in [
      (target.hex, NSColor.labelColor), (target.decimal, NSColor.secondaryLabelColor),
    ].enumerated() {
      (text as NSString).draw(
        in: CGRect(
          x: caption.minX, y: caption.minY + 5 + CGFloat(index) * 15, width: caption.width,
          height: 15),
        withAttributes: [.font: font, .foregroundColor: color, .paragraphStyle: style])
    }
  }
}

/// A read-only color chip for the contextual strip; empty until there is a sample.
@MainActor
final class SwatchView: NSView {
  var color: PixelColor? {
    didSet { if color != oldValue { needsDisplay = true } }
  }
  override var isFlipped: Bool { true }
  override var intrinsicContentSize: NSSize { NSSize(width: 22, height: 22) }
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    needsDisplay = true
  }
  override func draw(_ dirtyRect: NSRect) {
    if let color {
      fill(
        bounds, with: color, cell: 6,
        dark: effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua)
    }
    NSColor.tertiaryLabelColor.setFill()
    bounds.frame(withWidth: 1, using: .sourceOver)
  }
}
