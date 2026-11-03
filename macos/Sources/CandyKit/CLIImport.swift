import Foundation

/// The CLI's private handoff directory. Ordinary opened files are never consumed.
/// Keep the layout in sync with crates/candy/src/launcher.rs.
struct CLIImport: Sendable {
  let url: URL
  private let root: URL

  static var cacheRoot: URL {
    FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(
      "Library/Caches/me.told.candy.app/stdin", isDirectory: true)
  }

  static func recognize(_ url: URL, root: URL = cacheRoot) -> CLIImport? {
    guard url.isFileURL else { return nil }
    let root = root.standardizedFileURL
    let url = url.standardizedFileURL
    let directory = url.deletingLastPathComponent()
    let name = directory.lastPathComponent
    guard url.lastPathComponent == "stdin.png",
      directory.deletingLastPathComponent() == root,
      name.hasPrefix("import-"), name.count == 23,
      name.dropFirst(7).utf8.allSatisfy({
        (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0)
      }),
      privateItem(root, type: .typeDirectory),
      privateItem(directory, type: .typeDirectory),
      privateItem(url, type: .typeRegular)
    else { return nil }
    return CLIImport(url: url, root: root)
  }

  private static func privateItem(_ url: URL, type: FileAttributeType) -> Bool {
    guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path) else {
      return false
    }
    return attributes[.type] as? FileAttributeType == type
      && (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == geteuid()
      && ((attributes[.posixPermissions] as? NSNumber)?.intValue ?? 0o777) & 0o077 == 0
  }

  func remove() {
    guard Self.recognize(url, root: root) != nil else { return }
    // Remove only the transport file and its now-empty directory, never recurse
    // through a directory that might have gained unrelated contents.
    _ = url.withUnsafeFileSystemRepresentation { unlink($0!) }
    _ = url.deletingLastPathComponent().withUnsafeFileSystemRepresentation { rmdir($0!) }
  }
}
