import Foundation
import Testing

private let macos = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
  .deletingLastPathComponent().deletingLastPathComponent()

/// The Korean table translates exactly the keys the sources pass to `localized` and
/// `localizedFormat`, keeping each key's format placeholders.
@Test func koreanTableMatchesInterfaceText() throws {
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
  let url = macos.appendingPathComponent("Resources/ko.lproj/Localizable.strings")
  let table = try #require(NSDictionary(contentsOf: url) as? [String: String])
  #expect(Set(table.keys).subtracting(keys).isEmpty, "unused Korean entries")
  #expect(keys.subtracting(table.keys).isEmpty, "untranslated keys")
  let placeholder = try Regex(#"%(?:\d+\$)?(?:l{0,2}[diu]|@)"#)
  func placeholders(_ text: String) -> [String] {
    text.matches(of: placeholder).compactMap { $0.output[0].substring.map(String.init) }
  }
  for (key, value) in table { #expect(placeholders(key) == placeholders(value), "\(key)") }
}
