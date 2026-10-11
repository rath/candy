// Synthetic, reproducible fixtures. No user images or personal data.
import AppKit
import ImageIO
import UniformTypeIdentifiers

let destination = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
let width = 1000, height = 660
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
NSColor(calibratedRed: 0.97, green: 0.95, blue: 0.91, alpha: 1).setFill()
NSBezierPath(rect: NSRect(x: 0,y: 0,width: width,height: height)).fill()
func text(_ content: String, _ x: CGFloat, _ y: CGFloat, _ size: CGFloat, bold: Bool = false) {
    (content as NSString).draw(at: NSPoint(x:x, y:y), withAttributes: [
        .font: bold ? NSFont.boldSystemFont(ofSize: size) : NSFont.systemFont(ofSize: size),
        .foregroundColor: NSColor(calibratedRed: 0.18, green: 0.23, blue: 0.20, alpha: 1)])
}
text("A SMALL IMAGE, A FEW QUICK EDITS.", 56, 577, 16, bold: true)
text("Make room for the good bits.", 56, 503, 38, bold: true)
text("Candy · Synthetic test image", 56, 461, 19)
NSColor(calibratedRed: 0.83, green: 0.39, blue: 0.27, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 660,y: 128,width: 255,height: 274), xRadius: 28,yRadius: 28).fill()
NSColor(calibratedRed: 1.0, green: 0.79, blue: 0.35, alpha: 1).setFill()
NSBezierPath(ovalIn: NSRect(x: 701,y: 245,width: 160,height: 160)).fill()
text("Name: Sample Person", 56, 363, 27)
text("Email: sample@example.com", 56, 309, 27)
text("Phone: 000-1234-5678", 56, 255, 27)
text("안녕하세요. 캔디 이미지 편집기", 56, 190, 27)
text("Select. Highlight. Pixelate. Save a copy.", 56, 73, 22)
NSGraphicsContext.restoreGraphicsState()
let cgImage = bitmap.cgImage!
for (ext, type) in [("png", UTType.png), ("jpg", UTType.jpeg), ("heic", UTType.heic), ("avif", UTType("public.avif")!)] {
    let url = destination.appendingPathComponent("sample.\(ext)")
    guard let output = CGImageDestinationCreateWithURL(url as CFURL, type.identifier as CFString, 1, nil) else { fatalError("No encoder for \(ext)") }
    CGImageDestinationAddImage(output, cgImage, [kCGImageDestinationLossyCompressionQuality: 0.90] as CFDictionary)
    precondition(CGImageDestinationFinalize(output))
}
let rotated = CGImageDestinationCreateWithURL(destination.appendingPathComponent("orientation6.jpg") as CFURL, UTType.jpeg.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(rotated, cgImage, [kCGImagePropertyOrientation: 6] as CFDictionary)
precondition(CGImageDestinationFinalize(rotated))
print(destination.path)
