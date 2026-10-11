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
print(destination.path)
