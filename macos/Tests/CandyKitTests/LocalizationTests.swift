import Foundation
import Testing

private let macos = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
  .deletingLastPathComponent().deletingLastPathComponent()

/// Each translation table (every `.lproj` except the English development language) translates
/// exactly the keys the sources pass to `localized` and `localizedFormat`, keeping each key's
/// format placeholders.
@Test(arguments: ["ko", "ja"]) func tableMatchesInterfaceText(language: String) throws {
  let call = try Regex(#"localized(?:Format)?\("((?:[^"\\]|\\.)*)""#)
  let sources = macos.appendingPathComponent("Sources/CandyKit")
  var keys = Set<String>()
  for file in try FileManager.default.contentsOfDirectory(
    at: sources, includingPropertiesForKeys: nil) where file.pathExtension == "swift"
  {
    let text = try String(contentsOf: file, encoding: .utf8)
    for match in text.matches(of: call) {
      if let key = match.output[1].substring { keys.insert(String(key)) }
    }
  }
  #expect(keys.count > 50)
  let url = macos.appendingPathComponent("Resources/\(language).lproj/Localizable.strings")
  let table = try #require(NSDictionary(contentsOf: url) as? [String: String])
  #expect(Set(table.keys).subtracting(keys).isEmpty, "unused \(language) entries")
  #expect(keys.subtracting(table.keys).isEmpty, "untranslated \(language) keys")
  let placeholder = try Regex(#"%(?:\d+\$)?(?:l{0,2}[diu]|@)"#)
  func placeholders(_ text: String) -> [String] {
    text.matches(of: placeholder).compactMap { $0.output[0].substring.map(String.init) }
  }
  for (key, value) in table { #expect(placeholders(key) == placeholders(value), "\(key)") }
}

/// The build script ships every translation table and declares it to the bundle.
@Test func buildScriptShipsEveryTable() throws {
  let resources = macos.appendingPathComponent("Resources")
  let tables = try FileManager.default.contentsOfDirectory(at: resources, includingPropertiesForKeys: nil)
    .filter { $0.pathExtension == "lproj" }.map { $0.deletingPathExtension().lastPathComponent }
  #expect(tables.sorted() == ["en", "ja", "ko"])
  let script = try String(
    contentsOf: macos.deletingLastPathComponent().appendingPathComponent("scripts/build-app.sh"),
    encoding: .utf8)
  for table in tables {
    #expect(script.contains("macos/Resources/\(table).lproj"), "\(table).lproj is not copied")
    #expect(script.contains("<string>\(table)</string>"), "\(table) is not in CFBundleLocalizations")
  }
}
