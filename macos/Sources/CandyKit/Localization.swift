import Foundation

/// Interface text in the user's language. Keys are the English text; translations live in
/// `macos/Resources/<language>.lproj/Localizable.strings`, which `scripts/build-app.sh` copies
/// into the app. Outside the app bundle, as in tests, the English key is returned unchanged.
func localized(_ english: String) -> String {
  Bundle.main.localizedString(forKey: english, value: english, table: nil)
}
/// `localized(_:)` for a `String(format:)` template.
func localizedFormat(_ english: String, _ arguments: CVarArg...) -> String {
  String(format: localized(english), arguments: arguments)
}
