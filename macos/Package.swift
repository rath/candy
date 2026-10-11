import Foundation
// swift-tools-version: 6.0
import PackageDescription

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let target =
  ProcessInfo.processInfo.environment["CARGO_TARGET_DIR"].map {
    URL(fileURLWithPath: $0, relativeTo: root).standardizedFileURL
  } ?? root.appendingPathComponent("target")
let profile = ProcessInfo.processInfo.environment["CANDY_RUST_PROFILE"] ?? "debug"
let package = Package(
  name: "Candy",
  platforms: [.macOS(.v14)],
  products: [.executable(name: "Candy", targets: ["Candy"])],
  targets: [
    .target(name: "CCandy", publicHeadersPath: "include"),
    .target(
      name: "CandyKit", dependencies: ["CCandy"],
      linkerSettings: [
        .unsafeFlags(["-L", target.appendingPathComponent(profile).path]),
        .linkedLibrary("candy_ffi"), .linkedLibrary("iconv"),
        .linkedFramework("AppKit"), .linkedFramework("Foundation"),
        .linkedFramework("CoreFoundation"), .linkedFramework("CoreGraphics"),
        .linkedFramework("ImageIO"), .linkedFramework("Vision"),
      ]),
    .executableTarget(name: "Candy", dependencies: ["CandyKit"]),
    .testTarget(name: "CandyKitTests", dependencies: ["CandyKit"]),
  ]
)
