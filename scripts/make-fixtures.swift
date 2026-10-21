// Reproducible synthetic travel card. No user images or personal data.
import AppKit
import ImageIO
import UniformTypeIdentifiers

guard CommandLine.arguments.count == 2 else {
    fatalError("Usage: swift scripts/make-fixtures.swift <output-directory>")
}
let destination = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .appendingPathComponent("fixture-assets/coast.jpg")
guard let photograph = NSImage(contentsOf: source) else {
    fatalError("Missing fixture source: \(source.path)")
}
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)

func color(_ hex: UInt32) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255,
            green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: 1)
}

// Design coordinates are always 1000 × 660. Render the demo at 2× rather than
// enlarging the test bitmap so that type remains sharp in Retina app captures.
func travelCard(scale: Int) -> CGImage {
    let width = 1000 * scale, height = 660 * scale
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32)!
    NSGraphicsContext.saveGraphicsState()
    defer { NSGraphicsContext.restoreGraphicsState() }
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    let transform = NSAffineTransform()
    transform.scale(by: CGFloat(scale))
    transform.concat()
    color(0xFAF6ED).setFill()
    NSRect(x: 0, y: 0, width: 1000, height: 660).fill()

    // Aspect-fill the photograph without stretching it.
    let photoRect = NSRect(x: 520, y: 0, width: 480, height: 660)
    let ratio = max(photoRect.width / photograph.size.width,
                    photoRect.height / photograph.size.height)
    let crop = NSRect(x: (photograph.size.width - photoRect.width / ratio) / 2,
                      y: (photograph.size.height - photoRect.height / ratio) / 2,
                      width: photoRect.width / ratio, height: photoRect.height / ratio)
    photograph.draw(in: photoRect, from: crop, operation: .copy, fraction: 1)

    // Text uses top-left design coordinates. All names and booking details are
    // fictional; example.com is a reserved documentation domain.
    func text(_ content: String, top: CGFloat, size: CGFloat,
              weight: NSFont.Weight = .regular, ink: UInt32 = 0x163C3B) {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: size, weight: weight),
            .foregroundColor: color(ink),
        ]
        let string = content as NSString
        let bounds = string.size(withAttributes: attributes)
        precondition(bounds.width <= 432, "Fixture text overflows: \(content)")
        string.draw(at: NSPoint(x: 44, y: 660 - top - bounds.height), withAttributes: attributes)
    }

    text("WEEKEND NOTES / 03", top: 40, size: 17, weight: .semibold)
    text("Take the", top: 91, size: 66, weight: .bold)
    text("scenic route.", top: 163, size: 66, weight: .bold)
    text("안녕하세요. 지중해의 여름", top: 270, size: 27)
    text("JUN 14–17 · 3 NIGHTS", top: 316, size: 20, weight: .medium)

    color(0xC8D2C9).setFill()
    NSRect(x: 44, y: 660 - 373, width: 432, height: 1).fill()
    text("TRAVELER / 예약자", top: 396, size: 17, weight: .semibold)
    text("Alex Morgan", top: 427, size: 30, weight: .semibold)
    text("alex@example.com", top: 473, size: 24)
    text("Booking: DEMO-0826", top: 514, size: 24)
    text("A FICTIONAL GETAWAY · CANDY DEMO", top: 611, size: 16, weight: .medium)
    return bitmap.cgImage!
}

func write(_ image: CGImage, name: String, type: UTType,
           properties: [CFString: Any] = [:]) {
    let url = destination.appendingPathComponent(name)
    guard let output = CGImageDestinationCreateWithURL(url as CFURL, type.identifier as CFString, 1, nil)
    else { fatalError("No encoder for \(type.identifier)") }
    var metadata: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: 0.90]
    metadata.merge(properties) { _, new in new }
    CGImageDestinationAddImage(output, image, metadata as CFDictionary)
    precondition(CGImageDestinationFinalize(output), "Could not write \(url.path)")
}

let sample = travelCard(scale: 1)
for (ext, type) in [("png", UTType.png), ("jpg", UTType.jpeg),
                    ("heic", UTType.heic), ("avif", UTType("public.avif")!)] {
    write(sample, name: "sample.\(ext)", type: type)
}
write(sample, name: "orientation6.jpg", type: .jpeg, properties: [kCGImagePropertyOrientation: 6])
write(travelCard(scale: 2), name: "weekend-demo.png", type: .png)

// The same card as a vector document, drawn at 1200 × 792 so Candy rasterizes it
// at its own size. Text names system fonts, including a Korean line, so the
// fixture exercises resvg's font lookup rather than only its path renderer.
let vectorCard = """
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="792" viewBox="0 0 1200 792">
  <!-- Synthetic weekend travel card for Candy's tests. Every detail is fictional. -->
  <defs>
    <linearGradient id="sea" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#8CC5E8"/>
      <stop offset="1" stop-color="#1F6F9F"/>
    </linearGradient>
  </defs>
  <rect width="1200" height="792" fill="#FAF6ED"/>
  <rect x="624" width="576" height="792" fill="url(#sea)"/>
  <circle cx="1010" cy="200" r="70" fill="#FFD36B"/>
  <path d="M624 560 Q700 520 776 560 T928 560 T1080 560 T1232 560 V792 H624 Z" fill="#2B5F86" opacity="0.8"/>
  <path d="M624 640 Q700 600 776 640 T928 640 T1080 640 T1232 640 V792 H624 Z" fill="#163C3B" opacity="0.7"/>
  <g fill="#163C3B" font-family="Helvetica Neue, Apple SD Gothic Neo, sans-serif">
    <text x="53" y="64" font-size="20" font-weight="600">WEEKEND NOTES / 03</text>
    <text x="53" y="172" font-size="79" font-weight="700">Take the</text>
    <text x="53" y="258" font-size="79" font-weight="700">scenic route.</text>
    <text x="53" y="356" font-size="32">안녕하세요. 지중해의 여름</text>
    <text x="53" y="406" font-size="24" font-weight="500">JUN 14–17 · 3 NIGHTS</text>
    <rect x="53" y="448" width="518" height="1" fill="#C8D2C9"/>
    <text x="53" y="494" font-size="20" font-weight="600">TRAVELER / 예약자</text>
    <text x="53" y="544" font-size="36" font-weight="600">Alex Morgan</text>
    <text x="53" y="598" font-size="29">alex@example.com</text>
    <text x="53" y="648" font-size="29">Booking: DEMO-0826</text>
    <text x="53" y="754" font-size="19" font-weight="500">A FICTIONAL GETAWAY · CANDY DEMO</text>
  </g>
</svg>

"""
try vectorCard.write(to: destination.appendingPathComponent("sample.svg"),
                     atomically: true, encoding: .utf8)
print(destination.path)
