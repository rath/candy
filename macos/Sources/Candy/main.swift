import AppKit
import CandyKit

let application = NSApplication.shared
let delegate = CandyApplication()
application.delegate = delegate
#if DEBUG
  // A per-process visual-QA override; never changes the user's appearance settings.
  if let mode = ProcessInfo.processInfo.environment["CANDY_TEST_APPEARANCE"] {
    application.appearance = NSAppearance(named: mode == "dark" ? .darkAqua : .aqua)
  }
#endif
application.run()
