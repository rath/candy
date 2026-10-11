import AppKit
import UniformTypeIdentifiers

@MainActor
public final class CandyApplication: NSObject, NSApplicationDelegate, NSMenuItemValidation {
  private var editors: [UUID: EditorWindow] = [:]
  private var terminating = false
  private var queuedURLs: [URL] = []
  public override init() { super.init() }
  public func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.regular)
    installMenu()
    if editors.isEmpty { showEmpty() }
    NSApp.activate(ignoringOtherApps: true)
  }
  public func application(_ application: NSApplication, open urls: [URL]) { open(urls) }
  public func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool)
    -> Bool
  {
    if !hasVisibleWindows { showEmpty() }
    return true
  }
  public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    false
  }
  private func makeEditor() -> EditorWindow {
    let editor = EditorWindow()
    let id = UUID()
    editors[id] = editor
    editor.onClosed = { [weak self] in self?.editors.removeValue(forKey: id) }
    editor.onOpen = { [weak self] urls in self?.open(urls) }
    editor.onExtract = { [weak self] document, rendered, name in
      guard let self, !self.terminating else { return }
      let next = self.makeEditor()
      next.session.adopt(document, rendered: rendered, name: name)
      next.showWindow(nil)
    }
    return editor
  }
  private func showEmpty() {
    guard !terminating else { return }
    makeEditor().showWindow(nil)
  }
  private func open(_ urls: [URL]) {
    if terminating {
      queuedURLs.append(contentsOf: urls)
      return
    }
    for url in urls where url.isFileURL {
      let empty = editors.values.first {
        $0.session.state == nil && !$0.session.busy && $0.session.source == nil && !$0.prompting
      }
      if let empty {
        empty.showWindow(nil)
        empty.session.load(url)
      } else {
        makeEditor().open(url)
      }
    }
  }
  @objc public func openDocument(_ sender: Any?) {
    guard !terminating else { return }
    let panel = NSOpenPanel()
    panel.allowsMultipleSelection = true
    panel.canChooseDirectories = false
    panel.allowedContentTypes = CandyApplication.openableTypes
    panel.begin { [weak self] result in if result == .OK { self?.open(panel.urls) } }
  }
  /// Everything Candy opens: whatever Image I/O decodes. Identifiers the
  /// running macOS does not know, such as JPEG XL before macOS 26, drop out.
  /// Keep in step with LSItemContentTypes in scripts/build-app.sh.
  static let openableTypes: [UTType] = [
    "public.png", "public.jpeg", "org.webmproject.webp", "public.heic", "public.heif",
    "public.avif", "com.compuserve.gif", "public.tiff", "com.microsoft.bmp", "public.jpeg-2000",
    "public.jpeg-xl", "com.adobe.photoshop-image", "com.truevision.tga-image",
    "com.microsoft.ico", "com.apple.icns", "com.ilm.openexr-image", "public.radiance",
    "public.pbm", "public.camera-raw-image", "com.adobe.raw-image",
  ].compactMap { UTType($0) }
  private var activeEditor: EditorWindow? {
    (NSApp.keyWindow?.windowController ?? NSApp.mainWindow?.windowController) as? EditorWindow
  }
  @objc func undoDocument(_ sender: Any?) {
    if let text = NSApp.keyWindow?.firstResponder as? NSTextView {
      text.undoManager?.undo()
    } else {
      activeEditor?.undo(sender)
    }
  }
  @objc func redoDocument(_ sender: Any?) {
    if let text = NSApp.keyWindow?.firstResponder as? NSTextView {
      text.undoManager?.redo()
    } else {
      activeEditor?.redo(sender)
    }
  }
  public func validateMenuItem(_ item: NSMenuItem) -> Bool {
    guard !terminating else { return false }
    if item.action == #selector(undoDocument(_:)) || item.action == #selector(redoDocument(_:)) {
      let redo = item.action == #selector(redoDocument(_:))
      if let text = NSApp.keyWindow?.firstResponder as? NSTextView {
        return redo ? (text.undoManager?.canRedo ?? false) : (text.undoManager?.canUndo ?? false)
      }
      guard let editor = activeEditor, !editor.prompting, editor.session.ready else { return false }
      return redo ? editor.session.state?.canRedo == true : editor.session.state?.canUndo == true
    }
    return true
  }
  public func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    guard !terminating else { return .terminateLater }
    if editors.values.contains(where: { $0.session.busy || $0.prompting }) {
      NSSound.beep()
      return .terminateCancel
    }
    terminating = true
    // Stable identities: later Open events are queued until the decision ends.
    let documents = Array(editors.values)
    for editor in documents { editor.interactionBlocked = true }
    Task { @MainActor in self.confirmTermination(documents, index: 0) }
    return .terminateLater
  }
  private func confirmTermination(_ documents: [EditorWindow], index: Int) {
    guard index < documents.count else {
      NSApp.reply(toApplicationShouldTerminate: true)
      return
    }
    documents[index].prepareClose { [weak self] approved in
      guard let self else { return }
      if approved {
        self.confirmTermination(documents, index: index + 1)
      } else {
        self.terminating = false
        for editor in documents { editor.interactionBlocked = false }
        NSApp.reply(toApplicationShouldTerminate: false)
        let urls = self.queuedURLs
        self.queuedURLs = []
        self.open(urls)
      }
    }
  }
  private func installMenu() {
    let bar = NSMenu()
    func submenu(_ title: String) -> NSMenu {
      let item = NSMenuItem()
      item.title = title
      let menu = NSMenu(title: title)
      item.submenu = menu
      bar.addItem(item)
      return menu
    }
    func add(
      _ menu: NSMenu, _ title: String, _ action: Selector, _ key: String = "", shift: Bool = false,
      target: AnyObject? = nil
    ) {
      let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
      item.keyEquivalentModifierMask = shift ? [.command, .shift] : .command
      item.target = target
      menu.addItem(item)
    }
    let app = submenu("Candy")
    add(
      app, "About Candy", #selector(NSApplication.orderFrontStandardAboutPanel(_:)), target: NSApp)
    app.addItem(.separator())
    let services = NSMenu(title: "Services")
    let servicesItem = NSMenuItem(title: "Services", action: nil, keyEquivalent: "")
    servicesItem.submenu = services
    app.addItem(servicesItem)
    NSApp.servicesMenu = services
    app.addItem(.separator())
    add(app, "Hide Candy", #selector(NSApplication.hide(_:)), "h", target: NSApp)
    add(app, "Show All", #selector(NSApplication.unhideAllApplications(_:)), target: NSApp)
    app.addItem(.separator())
    add(app, "Quit Candy", #selector(NSApplication.terminate(_:)), "q", target: NSApp)
    let file = submenu("File")
    add(file, "Open…", #selector(openDocument(_:)), "o", target: self)
    file.addItem(.separator())
    add(file, "Close", #selector(NSWindow.performClose(_:)), "w")
    add(file, "Save…", #selector(EditorWindow.saveDocument(_:)), "s")
    add(file, "Save As…", #selector(EditorWindow.saveDocumentAs(_:)), "s", shift: true)
    let edit = submenu("Edit")
    add(edit, "Undo", #selector(undoDocument(_:)), "z", target: self)
    add(edit, "Redo", #selector(redoDocument(_:)), "z", shift: true, target: self)
    edit.addItem(.separator())
    add(edit, "Cut", #selector(NSText.cut(_:)), "x")
    add(edit, "Copy", #selector(NSText.copy(_:)), "c")
    add(edit, "Paste", #selector(NSText.paste(_:)), "v")
    add(edit, "Select All", #selector(NSText.selectAll(_:)), "a")
    let view = submenu("View")
    add(view, "Actual Size", #selector(EditorWindow.actualSize(_:)), "0")
    add(view, "Zoom to Fit", #selector(EditorWindow.zoomToFit(_:)), "9")
    add(view, "Show Tab Bar", #selector(NSWindow.toggleTabBar(_:)))
    let tools = submenu("Tools")
    for tool in Tool.allCases {
      let item = NSMenuItem(
        title: tool.title, action: #selector(EditorWindow.chooseTool(_:)),
        keyEquivalent: String(tool.rawValue + 1))
      item.tag = tool.rawValue
      tools.addItem(item)
    }
    let windows = submenu("Window")
    NSApp.windowsMenu = windows
    add(windows, "Minimize", #selector(NSWindow.performMiniaturize(_:)), "m")
    add(windows, "Zoom", #selector(NSWindow.performZoom(_:)))
    add(windows, "Move Tab to New Window", #selector(NSWindow.moveTabToNewWindow(_:)))
    add(windows, "Merge All Windows", #selector(NSWindow.mergeAllWindows(_:)))
    windows.addItem(.separator())
    add(windows, "Bring All to Front", #selector(NSApplication.arrangeInFront(_:)), target: NSApp)
    NSApp.mainMenu = bar
  }
}
