import Foundation
// swift-tools-version: 6.0
import PackageDescription

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let target =
  ProcessInfo.processInfo.environment["CARGO_TARGET_DIR"].map {
    URL(fileURLWithPath: $0, relativeTo: root).standardizedFileURL
  } ?? root.appendingPathComponent("target")
let profile = ProcessInfo.processInfo.environment["CANDY_RUST_PROFILE"] ?? "debug"
let rustBuildID = ProcessInfo.processInfo.environment["CANDY_RUST_BUILD_ID"] ?? "untracked"
let package = Package(
  name: "Candy",
  platforms: [.macOS(.v14)],
  products: [.executable(name: "Candy", targets: ["Candy"])],
  targets: [
    .target(
      name: "CCandy", publicHeadersPath: "include",
      cSettings: [.define("CANDY_RUST_BUILD_ID", to: "\"\(rustBuildID)\"")]),
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
