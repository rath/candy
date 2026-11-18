// Records one scene of the landing-page demo. Opens a rendered scene in a fresh Candy instance,
// moves the real pointer through the edits listed in the scene's targets JSON (render-scenes.sh),
// and writes Candy's windows to a movie, with an optional still of the finished window.
//
// The recording takes over the mouse and brings Candy to the front: run it only while nobody is
// using the Mac. --dry-run touches neither; it checks the window, the controls and the capture.
//
//   swiftc -O -swift-version 5 -o /tmp/candy-record scripts/demo/record.swift
//   /tmp/candy-record --app dist/Candy.app --image SCENE.png --targets SCENE.json --lang ko \
//     --out SCENE.mov [--still SCENE-still.png] [--dry-run]
//
// Needs Screen Recording and Accessibility access for the process that runs it.

import AVFoundation
import AppKit
import ApplicationServices
import ScreenCaptureKit

// MARK: Arguments

struct Options {
  var app = URL(fileURLWithPath: "dist/Candy.app")
  var image: URL!
  var targets: URL!
  var lang = "en"
  var out: URL!
  var still: URL?
  var dryRun = false
}

func parse() -> Options {
  var o = Options()
  var args = CommandLine.arguments.dropFirst()
  func value() -> String {
    guard let v = args.popFirst() else { fail("missing value") }
    return v
  }
  while let a = args.popFirst() {
    switch a {
    case "--app": o.app = URL(fileURLWithPath: value())
    case "--image": o.image = URL(fileURLWithPath: value())
    case "--targets": o.targets = URL(fileURLWithPath: value())
    case "--lang": o.lang = value()
    case "--out": o.out = URL(fileURLWithPath: value())
    case "--still": o.still = URL(fileURLWithPath: value())
    case "--dry-run": o.dryRun = true
    default: fail("unknown argument \(a)")
    }
  }
  guard o.image != nil, o.targets != nil, o.out != nil else {
    fail("usage: record --app APP --image PNG --targets JSON --lang LANG --out MOV [--still PNG] [--dry-run]")
  }
  return o
}

func fail(_ message: String) -> Never {
  FileHandle.standardError.write(Data("error: \(message)\n".utf8))
  exit(1)
}

func log(_ message: String) { print(message) }

func pause(_ seconds: Double) async { try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9)) }

struct Targets: Decodable {
  let scene: String
  let size: [Double]
  let mosaic: [[Double]]
  let highlight: [[Double]]
  let eyedropper: [Double]?
}

// MARK: Interface text

/// The app's own wording for `key` in `lang`, so controls are found by what they say.
func localizedText(_ key: String, lang: String, app: URL) -> String {
  guard lang != "en" else { return key }
  let table = app.appendingPathComponent("Contents/Resources/\(lang).lproj/Localizable.strings")
  guard let strings = NSDictionary(contentsOf: table) as? [String: String] else {
    fail("cannot read \(table.path)")
  }
  return strings[key] ?? key
}

// MARK: Accessibility

func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
  var value: CFTypeRef?
  guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
  return value as? T
}

func frame(of element: AXUIElement) -> CGRect? {
  guard let p: AXValue = attribute(element, kAXPositionAttribute),
    let s: AXValue = attribute(element, kAXSizeAttribute)
  else { return nil }
  var point = CGPoint.zero
  var size = CGSize.zero
  AXValueGetValue(p, .cgPoint, &point)
  AXValueGetValue(s, .cgSize, &size)
  return CGRect(origin: point, size: size)
}

func descendants(of element: AXUIElement, depth: Int = 0) -> [AXUIElement] {
  guard depth < 12, let children: [AXUIElement] = attribute(element, kAXChildrenAttribute) else { return [] }
  return children + children.flatMap { descendants(of: $0, depth: depth + 1) }
}

func label(of element: AXUIElement) -> String {
  let parts: [String?] = [attribute(element, kAXDescriptionAttribute), attribute(element, kAXTitleAttribute)]
  return parts.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " / ")
}

func role(of element: AXUIElement) -> String { attribute(element, kAXRoleAttribute) ?? "?" }

func find(in root: AXUIElement, role wanted: String? = nil, label text: String) -> AXUIElement? {
  descendants(of: root).first { element in
    (wanted == nil || role(of: element) == wanted)
      && ((attribute(element, kAXDescriptionAttribute) as String?) == text
        || (attribute(element, kAXTitleAttribute) as String?) == text)
  }
}

func press(_ element: AXUIElement) {
  AXUIElementPerformAction(element, kAXPressAction as CFString)
}

// MARK: Pointer

let source = CGEventSource(stateID: .hidSystemState)
nonisolated(unsafe) var pointer = CGEvent(source: nil)?.location ?? .zero

func post(_ type: CGEventType, at point: CGPoint) {
  guard let event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: .left)
  else { return }
  if type == .leftMouseDown || type == .leftMouseUp || type == .leftMouseDragged {
    event.setIntegerValueField(.mouseEventClickState, value: 1)
  }
  event.post(tap: .cghidEventTap)
}

/// Moves the pointer along a straight line with eased speed, as a hand would.
func glide(to target: CGPoint, duration: Double, dragging: Bool = false) async {
  let start = pointer
  let steps = max(2, Int(duration * 120))
  for step in 1...steps {
    let t = Double(step) / Double(steps)
    let eased = t * t * (3 - 2 * t)
    let point = CGPoint(x: start.x + (target.x - start.x) * eased, y: start.y + (target.y - start.y) * eased)
    post(dragging ? .leftMouseDragged : .mouseMoved, at: point)
    await pause(duration / Double(steps))
  }
  pointer = target
}

func click(_ point: CGPoint, travel: Double = 0.55) async {
  await glide(to: point, duration: travel)
  await pause(0.12)
  post(.leftMouseDown, at: point)
  await pause(0.08)
  post(.leftMouseUp, at: point)
  await pause(0.25)
}

func drag(from a: CGPoint, to b: CGPoint, duration: Double) async {
  await glide(to: a, duration: 0.55)
  await pause(0.15)
  post(.leftMouseDown, at: a)
  await pause(0.1)
  await glide(to: b, duration: duration, dragging: true)
  await pause(0.1)
  post(.leftMouseUp, at: b)
  await pause(0.3)
}

// MARK: Capture

final class MovieWriter: NSObject, SCStreamOutput, @unchecked Sendable {
  private let writer: AVAssetWriter
  private let input: AVAssetWriterInput
  private var started = false
  private(set) var frames = 0

  init(url: URL, width: Int, height: Int) throws {
    try? FileManager.default.removeItem(at: url)
    writer = try AVAssetWriter(outputURL: url, fileType: .mov)
    input = AVAssetWriterInput(
      mediaType: .video,
      outputSettings: [
        AVVideoCodecKey: AVVideoCodecType.h264,
        AVVideoWidthKey: width,
        AVVideoHeightKey: height,
        AVVideoCompressionPropertiesKey: [
          AVVideoAverageBitRateKey: 40_000_000,
          AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
          AVVideoExpectedSourceFrameRateKey: 60,
        ],
      ])
    input.expectsMediaDataInRealTime = true
    writer.add(input)
  }

  func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen, buffer.isValid,
      let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false)
        as? [[SCStreamFrameInfo: Any]],
      let raw = attachments.first?[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete
    else { return }
    if !started {
      writer.startWriting()
      writer.startSession(atSourceTime: buffer.presentationTimeStamp)
      started = true
    }
    if input.isReadyForMoreMediaData, input.append(buffer) { frames += 1 }
  }

  func finish() async throws {
    guard started else { throw NSError(domain: "record", code: 1, userInfo: [NSLocalizedDescriptionKey: "no frames captured"]) }
    input.markAsFinished()
    await writer.finishWriting()
    if writer.status != .completed { throw writer.error ?? NSError(domain: "record", code: 2) }
  }
}

// MARK: Scene

let coral = CGColor(srgbRed: 0xf2 / 255, green: 0xa2 / 255, blue: 0x8e / 255, alpha: 1)
let options = parse()
guard CGPreflightScreenCaptureAccess() else { fail("Screen Recording access is missing") }
guard AXIsProcessTrusted() else { fail("Accessibility access is missing") }
guard let targets = try? JSONDecoder().decode(Targets.self, from: Data(contentsOf: options.targets)) else {
  fail("cannot read \(options.targets.path)")
}
func text(_ key: String) -> String { localizedText(key, lang: options.lang, app: options.app) }

let configuration = NSWorkspace.OpenConfiguration()
configuration.createsNewApplicationInstance = true
configuration.activates = !options.dryRun
configuration.arguments = [
  "-AppleLanguages", "(\(options.lang))",
  "-NSRequiresAquaSystemAppearance", "YES",
  "-ApplePersistenceIgnoreState", "YES",
]
let running: NSRunningApplication
do {
  running = try await NSWorkspace.shared.open(
    [options.image], withApplicationAt: options.app, configuration: configuration)
} catch {
  fail("cannot open Candy: \(error.localizedDescription)")
}
defer { running.forceTerminate() }
let axApp = AXUIElementCreateApplication(running.processIdentifier)

// Wait for the image at Actual Size and for the window to stop resizing.
var window: AXUIElement?
var canvas: AXUIElement?
var settled: CGRect?
for _ in 0..<100 {
  await pause(0.1)
  guard let w = (attribute(axApp, kAXWindowsAttribute) as [AXUIElement]?)?.first,
    let c = find(in: w, role: "AXImage", label: text("Image canvas")),
    let status: String = attribute(c, kAXValueAttribute), status.contains("100%"),
    let f = frame(of: w)
  else { continue }
  if f == settled {
    window = w
    canvas = c
    break
  }
  settled = f
}
guard let window, let canvas, let windowFrame = frame(of: window), let canvasFrame = frame(of: canvas) else {
  running.forceTerminate()
  fail("Candy did not show the image at 100%")
}

/// Image pixels to screen points: the canvas centres the image, one pixel per backing pixel.
let backing = NSScreen.screens.first { $0.frame.width > 0 }?.backingScaleFactor ?? 2
let imageSize = CGSize(width: targets.size[0], height: targets.size[1])
let origin = CGPoint(
  x: canvasFrame.midX - imageSize.width / backing / 2, y: canvasFrame.midY - imageSize.height / backing / 2)
func screen(_ x: Double, _ y: Double) -> CGPoint {
  CGPoint(x: origin.x + x / backing, y: origin.y + y / backing)
}

let toolTitles = ["Select", "Mosaic", "Highlight", "Text mosaic", "Resize", "Flatten alpha", "Eyedropper"]
func tool(_ name: String) -> AXUIElement {
  guard let element = find(in: window, label: text(name)) else {
    running.forceTerminate()
    fail("no tool control for \(name)")
  }
  return element
}
func center(_ element: AXUIElement) -> CGPoint {
  guard let f = frame(of: element) else { fail("no frame for \(label(of: element))") }
  return CGPoint(x: f.midX, y: f.midY)
}

/// Sets a slider in the option bar through accessibility, before the recording starts.
func setSlider(_ title: String, to value: Double) {
  guard let slider = find(in: window, role: "AXSlider", label: text(title)) else {
    log("warning: no slider \(title)")
    return
  }
  AXUIElementSetAttributeValue(slider, kAXValueAttribute as CFString, value as CFNumber)
  // The readout beside the slider changes only when the slider's action ran, which is what
  // updates the tool; report it so a silent no-op shows up.
  guard let bounds = frame(of: slider) else { return }
  let readout = descendants(of: window)
    .filter { role(of: $0) == "AXStaticText" }
    .compactMap { element -> (CGRect, String)? in
      guard let f = frame(of: element), f.minX >= bounds.maxX - 1, abs(f.midY - bounds.midY) < 6,
        let v: String = attribute(element, kAXValueAttribute)
      else { return nil }
      return (f, v)
    }
    .min { $0.0.minX < $1.0.minX }
  log("slider \(title) -> \(value), readout \(readout?.1 ?? "none")")
}

log("window \(windowFrame), canvas \(canvasFrame), image origin \(origin), backing \(backing)")
for name in toolTitles {
  log("tool \(name): \(frame(of: tool(name)).map { "\($0)" } ?? "no frame")")
}

// The highlighter and block size of the earlier recordings: 40 px at 45 percent, 14 px blocks.
press(tool("Mosaic"))
await pause(0.3)
setSlider("Mosaic block size in pixels", to: 14)
press(tool("Highlight"))
await pause(0.3)
setSlider("Highlight width in pixels", to: 40)
setSlider("Highlight opacity", to: 0.45)
press(tool("Select"))
await pause(0.4)

let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
guard let scWindow = content.windows.first(where: {
    $0.owningApplication?.processID == running.processIdentifier && $0.windowLayer == 0
      && abs($0.frame.width - windowFrame.width) < 2
  }),
  let display = content.displays.first(where: { $0.frame.contains(CGPoint(x: windowFrame.midX, y: windowFrame.midY)) })
else {
  running.forceTerminate()
  fail("cannot find Candy's window for capture")
}
// The whole display minus every other window, rather than a filter that includes Candy: macOS
// marks an explicitly captured window as shared and draws a sharing badge over its window buttons.
let others = content.windows.filter { $0.owningApplication?.processID != running.processIdentifier }
let filter = SCContentFilter(display: display, excludingWindows: others)
let stream = SCStreamConfiguration()
stream.sourceRect = scWindow.frame.offsetBy(dx: -display.frame.minX, dy: -display.frame.minY)
stream.width = Int(scWindow.frame.width * CGFloat(filter.pointPixelScale))
stream.height = Int(scWindow.frame.height * CGFloat(filter.pointPixelScale))
stream.showsCursor = true
// Window shadows would darken the coral in the corners.
stream.ignoreShadowsDisplay = true
// The landing page's coral stage shows through the rounded corners. The configuration does not
// retain the colour, so it lives in a constant for the whole run.
stream.backgroundColor = coral
stream.minimumFrameInterval = CMTime(value: 1, timescale: 60)
stream.pixelFormat = kCVPixelFormatType_32BGRA
stream.colorSpaceName = CGColorSpace.sRGB
stream.queueDepth = 8
log("capture \(stream.width) x \(stream.height) of window \(scWindow.windowID)")

let writer = try MovieWriter(url: options.out, width: stream.width, height: stream.height)
let capture = SCStream(filter: filter, configuration: stream, delegate: nil)
try capture.addStreamOutput(writer, type: .screen, sampleHandlerQueue: DispatchQueue(label: "capture"))

/// A point just outside the window's lower right corner, where the pointer waits off camera.
let offstage = CGPoint(x: windowFrame.maxX + 40, y: windowFrame.maxY - 60)
if !options.dryRun {
  // macOS may keep a launched app in the background. A click on the window title activates it
  // off camera, so the first toolbar click in the recording selects a tool instead.
  // Asked through accessibility: NSRunningApplication.isActive only updates on a run loop.
  func frontmost() -> Bool { (attribute(axApp, kAXFrontmostAttribute) as Bool?) ?? false }
  if !frontmost() {
    await click(CGPoint(x: windowFrame.midX - 120, y: windowFrame.minY + 20), travel: 0.2)
    await pause(0.4)
  }
  guard frontmost() else {
    running.forceTerminate()
    fail("Candy did not become the active app")
  }
  await glide(to: offstage, duration: 0.2)
}
// Spelled out: in top-level code `try await capture.startCapture()` resolves to the overload with
// an optional completion handler, which drops the error.
try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
  capture.startCapture { error in
    if let error { done.resume(throwing: error) } else { done.resume() }
  }
}
await pause(0.6)

func padded(_ box: [Double], _ pad: Double) -> (CGPoint, CGPoint) {
  (screen(box[0] - pad, box[1] - pad), screen(box[0] + box[2] + pad, box[1] + box[3] + pad))
}
func mosaic(_ box: [Double]) async {
  let (a, b) = padded(box, 6)
  await drag(from: a, to: b, duration: 0.7)
  guard let apply = find(in: window, role: "AXButton", label: text("Apply mosaic")) else {
    fail("no Apply mosaic button")
  }
  await click(center(apply), travel: 0.5)
  await pause(0.45)
}
func highlight(_ box: [Double]) async {
  let y = box[1] + box[3] / 2
  await drag(from: screen(box[0] - 4, y), to: screen(box[0] + box[2] + 4, y), duration: 0.5 + box[2] / 1600)
}

if options.dryRun {
  await pause(0.6)
} else {
  await pause(0.3)
  await click(center(tool("Mosaic")))
  for box in targets.mosaic { await mosaic(box) }
  await click(center(tool("Highlight")))
  for box in targets.highlight { await highlight(box) }
  if let avatar = targets.eyedropper {
    await click(center(tool("Eyedropper")))
    // Off the letter, inside the coloured disc.
    let spot = screen(avatar[0] + avatar[2] * 0.27, avatar[1] + avatar[3] * 0.3)
    await glide(to: CGPoint(x: spot.x + 60, y: spot.y + 70), duration: 0.5)
    await glide(to: spot, duration: 0.7)
    await pause(0.5)
    await click(spot, travel: 0)
    await pause(1.3)
  }
  await glide(to: offstage, duration: 0.6)
  await pause(0.8)
}

try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
  capture.stopCapture { error in
    if let error { done.resume(throwing: error) } else { done.resume() }
  }
}
try await writer.finish()
log("wrote \(options.out.path) (\(writer.frames) frames)")

if let still = options.still {
  let shot = Process()
  shot.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
  shot.arguments = ["-x", "-o", "-l\(scWindow.windowID)", still.path]
  try shot.run()
  shot.waitUntilExit()
  log("wrote \(still.path)")
}

if options.dryRun {
  for element in descendants(of: window) where ["AXSlider", "AXButton", "AXRadioButton", "AXImage"].contains(role(of: element)) {
    let value: Any? = attribute(element, kAXValueAttribute) as CFTypeRef?
    log("\(role(of: element)) \(label(of: element)) value=\(value.map { "\($0)" } ?? "-") \(frame(of: element).map { "\($0)" } ?? "")")
  }
}
running.forceTerminate()
