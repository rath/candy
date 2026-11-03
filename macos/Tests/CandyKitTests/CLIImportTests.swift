import AppKit
import Testing

@testable import CandyKit

private var fixture: URL {
  URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(
      "crates/candy-macos/tests/fixtures/sample.png")
}

private func prepare(_ root: URL, bytes: Data? = nil) throws -> CLIImport {
  let directory = root.appendingPathComponent("import-0123456789abcdef", isDirectory: true)
  try FileManager.default.createDirectory(
    at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
  let url = directory.appendingPathComponent("stdin.png")
  try (bytes ?? Data(contentsOf: fixture)).write(to: url)
  try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
  return try #require(CLIImport.recognize(url, root: root))
}

@Test func importsRejectOrdinaryFilesAliasesAndNonPrivateDirectories() throws {
  let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  defer { try? FileManager.default.removeItem(at: root) }
  let transfer = try prepare(root)
  #expect(CLIImport.recognize(fixture, root: root) == nil)
  let directory = transfer.url.deletingLastPathComponent()
  try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: directory.path)
  #expect(CLIImport.recognize(transfer.url, root: root) == nil)
  transfer.remove()
  #expect(FileManager.default.fileExists(atPath: transfer.url.path))
  try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
  try FileManager.default.removeItem(at: transfer.url)
  try FileManager.default.createSymbolicLink(at: transfer.url, withDestinationURL: fixture)
  #expect(CLIImport.recognize(transfer.url, root: root) == nil)
  transfer.remove()
  #expect(FileManager.default.fileExists(atPath: transfer.url.path))
  #expect(FileManager.default.fileExists(atPath: fixture.path))
}

@Test func cleanupNeverRemovesUnrelatedDirectoryContents() throws {
  let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  defer { try? FileManager.default.removeItem(at: root) }
  let transfer = try prepare(root)
  let other = transfer.url.deletingLastPathComponent().appendingPathComponent("keep.txt")
  try Data("keep".utf8).write(to: other)
  transfer.remove()
  #expect(!FileManager.default.fileExists(atPath: transfer.url.path))
  #expect(try Data(contentsOf: other) == Data("keep".utf8))
}

@Suite(.serialized) @MainActor struct CLIImportSessionTests {
  private func settled(_ session: DocumentSession) async throws {
    for _ in 0..<500 {
      if !session.busy { return }
      try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("Import did not finish")
  }

  @Test func importsStartCleanAndCanSaveAfterTransportWasDeleted() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let transfer = try prepare(root)
    let session = DocumentSession()
    session.load(transfer.url, transfer: transfer)
    try await settled(session)
    #expect(session.error == nil)
    #expect(session.state?.width == 1000 && session.state?.height == 660)
    #expect(session.name == "stdin.png" && session.source == nil && session.output == nil)
    #expect(!session.dirty)
    #expect(!FileManager.default.fileExists(atPath: transfer.url.deletingLastPathComponent().path))
    let output = root.appendingPathComponent("saved.png")
    let saved = await withCheckedContinuation { continuation in
      session.save(output) { continuation.resume(returning: $0) }
    }
    #expect(saved && session.output == output && !session.dirty)
    session.apply(.rotate(clockwise: true))
    try await settled(session)
    #expect(session.dirty && session.state?.width == 660)
    session.apply(.undo)
    try await settled(session)
    #expect(!session.dirty)
  }

  @Test func failedOrAbandonedLoadsStillRemoveTheirTransport() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let broken = try prepare(root, bytes: Data("not an image".utf8))
    let failed = DocumentSession()
    failed.load(broken.url, transfer: broken)
    try await settled(failed)
    #expect(failed.state == nil && failed.error != nil)
    #expect(!FileManager.default.fileExists(atPath: broken.url.path))

    let transfer = try prepare(root)
    let closed = DocumentSession()
    closed.load(transfer.url, transfer: transfer)
    closed.invalidate()
    for _ in 0..<500 {
      if !FileManager.default.fileExists(atPath: transfer.url.path) { break }
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(!FileManager.default.fileExists(atPath: transfer.url.path))
    #expect(closed.state == nil)
  }

  @Test func anUneditedImportClosesWithoutASavePrompt() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let transfer = try prepare(root)
    _ = NSApplication.shared
    let editor = EditorWindow()
    editor.session.load(transfer.url, transfer: transfer)
    try await settled(editor.session)
    var approved = false
    editor.prepareClose { approved = $0 }
    #expect(approved && !editor.prompting)
    editor.session.invalidate()
  }
}
