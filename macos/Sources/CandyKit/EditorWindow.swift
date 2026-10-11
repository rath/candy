import AppKit
import UniformTypeIdentifiers

@MainActor
final class EditorWindow: NSWindowController, NSWindowDelegate, NSToolbarDelegate,
  NSMenuItemValidation, NSTextFieldDelegate
{
  let session = DocumentSession()
  let canvas = CanvasView()
  var onClosed: (() -> Void)?
  var onExtract: ((RustDocument, RenderedDocument, String) -> Void)?
  var onOpen: (([URL]) -> Void)?
  private let options = NSStackView()
  private let status = NSTextField(labelWithString: "Ready")
  private let progress = NSProgressIndicator()
  private let dismiss = NSButton(title: "Dismiss", target: nil, action: nil)
  private let empty = NSStackView()
  private let tools = NSSegmentedControl(
    images: Tool.allCases.map {
      NSImage(systemSymbolName: $0.symbol, accessibilityDescription: $0.title) ?? NSImage()
    }, trackingMode: .selectOne, target: nil, action: nil)
  private var actionButtons: [NSButton] = []
  private var tool: Tool = .select
  private var block: Double = 12
  private var brushWidth: Double = 28
  private var opacity: Double = 0.4
  private var color = NSColor.systemYellow
  private var background = NSColor.white
  private var widthField: NSTextField?
  private var heightField: NSTextField?
  private var ratio: NSButton?
  private var colorWell: NSColorWell?
  private var slider: NSSlider?
  private var alphaSlider: NSSlider?
  private var sliderValues: [(NSSlider, NSTextField)] = []
  var interactionBlocked = false {
    didSet {
      if interactionBlocked { canvas.cancel() }
      refresh()
    }
  }
  private var documentActionsEnabled: Bool { session.ready && !prompting && !interactionBlocked }
  private var regionActionsEnabled: Bool {
    documentActionsEnabled && !(tool == .text && session.detecting)
  }
  private(set) var prompting = false
  private var allowClose = false
  private var shownRevision: UInt64?
  private var shownSize = CGSize.zero
  private var userSized = false
  private var showWhenLoaded = false
  private var observedGeneration: UInt64 = 0

  init() {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1120, height: 780),
      styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
    )
    window.title = "Candy"
    window.tabbingIdentifier = "me.told.candy.app.image"
    window.tabbingMode = .automatic
    window.isReleasedWhenClosed = false
    super.init(window: window)
    window.delegate = self
    window.center()
    let toolbar = NSToolbar(identifier: "CandyTools")
    toolbar.delegate = self
    toolbar.displayMode = .iconOnly
    toolbar.allowsUserCustomization = false
    window.toolbar = toolbar
    window.toolbarStyle = .unified
    // A content minimum: `minSize` would be converted with the titlebar height before the
    // toolbar lays out, which later adds 24 pt. 560 pt fits the widest contextual strip
    // (Text mosaic) and the tool selector beside a truncated title.
    window.contentMinSize = NSSize(width: 560, height: 268)
    tools.target = self
    tools.action = #selector(changeTool)
    tools.selectedSegment = 0
    tools.setAccessibilityLabel("Editing tool")
    for tool in Tool.allCases { tools.setToolTip(tool.title, forSegment: tool.rawValue) }
    buildContent()
    configureOptions()
    session.onChange = { [weak self] in self?.refresh() }
    session.onRegions = { [weak self] regions in self?.canvas.regions = Regions(boxes: regions) }
    canvas.onChange = { [weak self] in self?.refreshStatus() }
    canvas.onStroke = { [weak self] points in
      guard let self, self.documentActionsEnabled, let c = self.color.usingColorSpace(.sRGB) else {
        return
      }
      self.session.apply(
        .highlight(
          points, Float(self.brushWidth),
          [
            UInt8((c.redComponent * 255).rounded()), UInt8((c.greenComponent * 255).rounded()),
            UInt8((c.blueComponent * 255).rounded()), UInt8((self.opacity * 255).rounded()),
          ]))
    }
    refresh()
  }
  required init?(coder: NSCoder) { fatalError("Programmatic window") }
  func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    [.init("tools"), .flexibleSpace]
  }
  func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
    [.flexibleSpace, .init("tools"), .flexibleSpace]
  }
  func toolbar(
    _ toolbar: NSToolbar, itemForItemIdentifier id: NSToolbarItem.Identifier,
    willBeInsertedIntoToolbar: Bool
  ) -> NSToolbarItem? {
    guard id.rawValue == "tools" else { return nil }
    let item = NSToolbarItem(itemIdentifier: id)
    item.label = "Tools"
    item.view = tools
    return item
  }
  private func buildContent() {
    guard let content = window?.contentView else { return }
    options.orientation = .horizontal
    options.spacing = 10
    options.edgeInsets = NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12)
    options.alignment = .centerY
    let scroller = NSScrollView()
    scroller.drawsBackground = false
    scroller.hasHorizontalScroller = true
    scroller.autohidesScrollers = true
    scroller.documentView = options
    options.translatesAutoresizingMaskIntoConstraints = false
    NSLayoutConstraint.activate([
      options.leadingAnchor.constraint(equalTo: scroller.contentView.leadingAnchor),
      options.topAnchor.constraint(equalTo: scroller.contentView.topAnchor),
      options.heightAnchor.constraint(equalTo: scroller.contentView.heightAnchor),
    ])
    let bottom = NSStackView()
    bottom.orientation = .horizontal
    bottom.spacing = 8
    bottom.edgeInsets = NSEdgeInsets(top: 6, left: 12, bottom: 6, right: 12)
    status.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    status.lineBreakMode = .byTruncatingMiddle
    progress.style = .spinning
    progress.controlSize = .small
    progress.isDisplayedWhenStopped = false
    dismiss.target = self
    dismiss.action = #selector(dismissError)
    dismiss.bezelStyle = .rounded
    dismiss.controlSize = .small
    bottom.addArrangedSubview(status)
    bottom.addArrangedSubview(progress)
    bottom.addArrangedSubview(dismiss)
    status.setContentHuggingPriority(.defaultLow, for: .horizontal)
    let line = NSBox()
    line.boxType = .separator
    for view in [scroller, canvas, line, bottom] {
      view.translatesAutoresizingMaskIntoConstraints = false
      content.addSubview(view)
    }
    NSLayoutConstraint.activate([
      scroller.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      scroller.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      scroller.topAnchor.constraint(equalTo: content.topAnchor),
      scroller.heightAnchor.constraint(equalToConstant: 54),
      canvas.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      canvas.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      canvas.topAnchor.constraint(equalTo: scroller.bottomAnchor),
      canvas.bottomAnchor.constraint(equalTo: line.topAnchor),
      line.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      line.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      line.bottomAnchor.constraint(equalTo: bottom.topAnchor),
      bottom.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      bottom.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      bottom.bottomAnchor.constraint(equalTo: content.bottomAnchor),
      bottom.heightAnchor.constraint(equalToConstant: 32),
    ])
    empty.orientation = .vertical
    empty.spacing = 12
    empty.translatesAutoresizingMaskIntoConstraints = false
    let heading = NSTextField(labelWithString: "A small image. A quick edit.")
    heading.font = .systemFont(ofSize: 22, weight: .semibold)
    empty.addArrangedSubview(heading)
    empty.addArrangedSubview(
      NSTextField(labelWithString: "Open an image or drop one into this window."))
    let open = NSButton(
      title: "Open image…", target: nil, action: #selector(CandyApplication.openDocument(_:)))
    open.bezelStyle = .rounded
    empty.addArrangedSubview(open)
    let formats = NSTextField(
      labelWithString: "PNG · JPEG · WebP · HEIC · AVIF · TIFF · GIF · BMP · PSD · RAW and more")
    formats.textColor = .secondaryLabelColor
    formats.font = .systemFont(ofSize: 11)
    empty.addArrangedSubview(formats)
    canvas.addSubview(empty)
    NSLayoutConstraint.activate([
      empty.centerXAnchor.constraint(equalTo: canvas.centerXAnchor),
      empty.centerYAnchor.constraint(equalTo: canvas.centerYAnchor),
    ])
    canvas.setAccessibilityElement(true)
    canvas.setAccessibilityRole(.image)
    canvas.setAccessibilityLabel("Image canvas")
    window?.registerForDraggedTypes([.fileURL])
  }
  private func label(_ title: String) {
    let l = NSTextField(labelWithString: title)
    l.font = .systemFont(ofSize: 12)
    options.addArrangedSubview(l)
  }
  @discardableResult private func button(_ title: String, _ action: Selector) -> NSButton {
    let b = NSButton(title: title, target: self, action: action)
    b.bezelStyle = .rounded
    options.addArrangedSubview(b)
    actionButtons.append(b)
    return b
  }
  private func addSlider(
    _ value: Double, _ min: Double, _ max: Double, _ action: Selector, title: String
  ) -> NSSlider {
    let s = NSSlider(value: value, minValue: min, maxValue: max, target: self, action: action)
    s.widthAnchor.constraint(equalToConstant: 115).isActive = true
    s.setAccessibilityLabel(title)
    s.toolTip = title
    options.addArrangedSubview(s)
    let readout = NSTextField(labelWithString: "")
    readout.font = .monospacedDigitSystemFont(ofSize: 12, weight: .regular)
    readout.alignment = .right
    readout.widthAnchor.constraint(equalToConstant: 36).isActive = true
    options.addArrangedSubview(readout)
    sliderValues.append((s, readout))
    updateSliderValues()
    return s
  }
  private func updateSliderValues() {
    for (slider, readout) in sliderValues {
      readout.stringValue =
        slider.maxValue <= 1
        ? "\(Int((slider.doubleValue * 100).rounded()))%" : "\(Int(slider.doubleValue.rounded()))"
    }
  }
  private func field(_ value: Int, title: String) -> NSTextField {
    let f = NSTextField(string: String(value))
    f.widthAnchor.constraint(equalToConstant: 76).isActive = true
    f.delegate = self
    f.target = self
    f.action = #selector(sizeChanged(_:))
    f.setAccessibilityLabel(title)
    options.addArrangedSubview(f)
    return f
  }
  private func configureOptions() {
    for view in options.arrangedSubviews {
      options.removeArrangedSubview(view)
      view.removeFromSuperview()
    }
    actionButtons = []
    sliderValues = []
    widthField = nil
    heightField = nil
    ratio = nil
    colorWell = nil
    slider = nil
    alphaSlider = nil
    switch tool {
    case .select:
      label("Drag a region. Move it or adjust its corners.")
      button("Extract to new image", #selector(extractSelection))
    case .mosaic:
      label("Block px")
      slider = addSlider(
        block, 2, 96, #selector(blockChanged), title: "Mosaic block size in pixels")
      button("Apply mosaic", #selector(applyTool))
    case .highlight:
      let well = NSColorWell()
      well.color = color
      well.target = self
      well.action = #selector(colorChanged)
      well.setAccessibilityLabel("Highlight color")
      options.addArrangedSubview(well)
      colorWell = well
      label("Width px")
      slider = addSlider(
        brushWidth, 2, 100, #selector(brushChanged), title: "Highlight width in pixels")
      label("Opacity")
      alphaSlider = addSlider(
        opacity, 16.0 / 255, 220.0 / 255, #selector(brushChanged), title: "Highlight opacity")
    case .text:
      button("Detect text", #selector(detectText))
      label("Block px")
      slider = addSlider(
        block, 2, 96, #selector(blockChanged), title: "Text mosaic block size in pixels")
      button("Apply to all", #selector(applyTool))
      button("Remove box", #selector(removeBox))
    case .resize:
      label("Width")
      widthField = field(session.state?.width ?? 1, title: "Width in pixels")
      label("Height")
      heightField = field(session.state?.height ?? 1, title: "Height in pixels")
      let r = NSButton(checkboxWithTitle: "Keep ratio", target: nil, action: nil)
      r.state = .on
      options.addArrangedSubview(r)
      ratio = r
      button("Apply resize", #selector(applyTool))
    case .flatten:
      label("Replace transparency with")
      let well = NSColorWell()
      well.color = background
      well.target = self
      well.action = #selector(colorChanged)
      well.setAccessibilityLabel("Background color")
      options.addArrangedSubview(well)
      colorWell = well
      button("Flatten alpha", #selector(applyTool))
    }
    refresh()
  }
  func selectTool(_ next: Tool) {
    guard !prompting, !interactionBlocked, !session.busy else { return }
    tool = next
    tools.selectedSegment = next.rawValue
    canvas.tool = next
    configureOptions()
  }
  @objc private func changeTool() {
    if let tool = Tool(rawValue: tools.selectedSegment) { selectTool(tool) }
  }
  @objc func chooseTool(_ sender: NSMenuItem) {
    if let tool = Tool(rawValue: sender.tag) { selectTool(tool) }
  }
  @objc private func blockChanged() {
    block = slider?.doubleValue.rounded() ?? 12
    updateSliderValues()
  }
  @objc private func brushChanged() {
    brushWidth = slider?.doubleValue ?? 28
    opacity = alphaSlider?.doubleValue ?? 0.4
    canvas.brushWidth = brushWidth
    canvas.brushColor = color.withAlphaComponent(opacity)
    updateSliderValues()
  }
  @objc private func colorChanged() {
    if tool == .flatten {
      background = colorWell?.color ?? .white
    } else {
      color = colorWell?.color ?? .systemYellow
      brushChanged()
    }
  }
  @objc private func sizeChanged(_ sender: NSTextField) {
    guard ratio?.state == .on, let state = session.state, let value = UInt32(sender.stringValue),
      value > 0, value <= 65535
    else { return }
    if sender === widthField {
      heightField?.stringValue = String(
        max(1, Int((Double(value) * Double(state.height) / Double(state.width)).rounded())))
    } else {
      widthField?.stringValue = String(
        max(1, Int((Double(value) * Double(state.width) / Double(state.height)).rounded())))
    }
  }
  func controlTextDidEndEditing(_ notification: Notification) {
    if let field = notification.object as? NSTextField { sizeChanged(field) }
  }
  @objc private func applyTool() {
    guard regionActionsEnabled else { return }
    window?.makeFirstResponder(canvas)
    switch tool {
    case .mosaic: if let r = canvas.regions.selection { session.apply(.mosaic([r], UInt32(block))) }
    case .text:
      if !canvas.regions.boxes.isEmpty {
        session.apply(.mosaic(canvas.regions.boxes, UInt32(block)))
      }
    case .resize:
      guard let w = UInt32(widthField?.stringValue ?? ""),
        let h = UInt32(heightField?.stringValue ?? ""), w > 0, h > 0, w <= 65535, h <= 65535
      else {
        session.fail(EngineError(message: "Enter dimensions between 1 and 65535 pixels."))
        return
      }
      session.apply(.resize(w, h))
    case .flatten:
      if let c = background.usingColorSpace(.sRGB) {
        session.apply(
          .flatten(
            UInt8((c.redComponent * 255).rounded()), UInt8((c.greenComponent * 255).rounded()),
            UInt8((c.blueComponent * 255).rounded())))
      }
    default: break
    }
  }
  @objc private func extractSelection() {
    if let r = canvas.regions.selection {
      session.extract(r) { [weak self] d, p, n in self?.onExtract?(d, p, n) }
    }
  }
  @objc private func detectText() {
    if documentActionsEnabled {
      canvas.cancel()
      session.detect()
    }
  }
  @objc private func removeBox() { if regionActionsEnabled { canvas.regions.removeActive() } }
  @objc private func dismissError() { session.clearError() }
  @objc func undo(_ sender: Any?) {
    guard documentActionsEnabled, session.state?.canUndo == true else { return }
    session.apply(.undo)
  }
  @objc func redo(_ sender: Any?) {
    guard documentActionsEnabled, session.state?.canRedo == true else { return }
    session.apply(.redo)
  }
  @objc func actualSize(_ sender: Any?) { canvas.actualSize() }
  @objc func zoomToFit(_ sender: Any?) { canvas.fit() }
  @objc func saveDocument(_ sender: Any?) { if documentActionsEnabled { save(as: false) { _ in } } }
  @objc func saveDocumentAs(_ sender: Any?) {
    if documentActionsEnabled { save(as: true) { _ in } }
  }
  private func save(as saveAs: Bool, completion: @escaping @MainActor @Sendable (Bool) -> Void) {
    guard let window, session.ready, !prompting else {
      completion(false)
      return
    }
    if !saveAs, let url = session.output {
      session.save(url, completion: completion)
      return
    }
    prompting = true
    refresh()
    let panel = NSSavePanel()
    panel.title = "Save image copy"
    panel.canCreateDirectories = true
    panel.directoryURL = (session.output ?? session.source)?.deletingLastPathComponent()
    panel.nameFieldStringValue =
      session.output?.lastPathComponent
      ?? (session.source == nil
        ? session.name : (session.name as NSString).deletingPathExtension + "-edited.png")
    panel.isExtensionHidden = false
    panel.canSelectHiddenExtension = false
    panel.accessoryView = SaveFormatView(panel: panel)
    panel.beginSheetModal(for: window) { [weak self] response in
      guard let self else {
        completion(false)
        return
      }
      self.prompting = false
      self.refresh()
      guard response == .OK, let url = panel.url else {
        completion(false)
        return
      }
      self.session.save(url, completion: completion)
    }
  }
  func prepareClose(_ completion: @escaping @MainActor @Sendable (Bool) -> Void) {
    guard let window, !prompting, !session.busy else {
      completion(false)
      return
    }
    guard session.dirty else {
      completion(true)
      return
    }
    prompting = true
    refresh()
    let alert = NSAlert()
    alert.messageText = "Save changes to \"\(session.name)\"?"
    alert.informativeText = "Your changes will be lost if you don’t save them."
    alert.addButton(withTitle: session.output == nil ? "Save copy…" : "Save")
    alert.addButton(withTitle: "Cancel")
    alert.addButton(withTitle: "Discard")
    alert.beginSheetModal(for: window) { [weak self] response in
      guard let self else {
        completion(false)
        return
      }
      self.prompting = false
      self.refresh()
      switch response {
      case .alertFirstButtonReturn: self.save(as: false, completion: completion)
      case .alertThirdButtonReturn: completion(true)
      default: completion(false)
      }
    }
  }
  func windowShouldClose(_ sender: NSWindow) -> Bool {
    if interactionBlocked { return false }
    if allowClose { return true }
    if session.busy {
      session.message = "Wait for the current edit or save to finish."
      refreshStatus()
      return false
    }
    prepareClose { [weak self] success in
      if success, let self {
        self.allowClose = true
        self.close()
      }
    }
    return false
  }
  func windowWillClose(_ notification: Notification) {
    session.invalidate()
    onClosed?()
  }
  func windowDidResize(_ notification: Notification) {
    canvas.needsDisplay = true
    refreshStatus()
  }
  /// Loads `url` and shows the window once the load has finished, so it appears already sized
  /// to the image, or with the error, instead of resizing in front of the user.
  func open(_ url: URL) {
    showWhenLoaded = true
    session.load(url)
  }
  func windowDidEndLiveResize(_ notification: Notification) { userSized = true }
  func windowDidChangeBackingProperties(_ notification: Notification) {
    canvas.needsDisplay = true
    refreshStatus()
  }
  /// Sizes the window for its first image so Fit shows it at Actual Size, or as large as the
  /// screen allows. Tabs share one frame, so a tabbed window keeps it, as does a full-screen
  /// window or one the user has resized.
  private func fitWindow(to image: CGSize) {
    guard let window, !userSized, !window.styleMask.contains(.fullScreen),
      (window.tabbedWindows?.count ?? 1) <= 1, let screen = window.screen ?? NSScreen.main
    else { return }
    window.contentView?.layoutSubtreeIfNeeded()
    let frame = EditorWindow.compactFrame(
      image: image, scale: screen.backingScaleFactor,
      decor: CGSize(
        width: window.frame.width - canvas.bounds.width,
        height: window.frame.height - canvas.bounds.height),
      around: window.frame, minimum: window.minSize, visible: screen.visibleFrame)
    if window.isVisible {
      window.animator().setFrame(frame, display: true)
    } else {
      window.setFrame(frame, display: false)
    }
  }
  /// A window frame whose canvas holds `image` pixels at `scale` pixels per point plus the Fit
  /// margin, shrunk proportionally to fit `visible` and centered on the current frame. `decor` is
  /// the frame space outside the canvas: titlebar, toolbar, option and status strips.
  nonisolated static func compactFrame(
    image: CGSize, scale: CGFloat, decor: CGSize, around frame: CGRect, minimum: CGSize,
    visible: CGRect
  ) -> CGRect {
    let margin = CanvasView.fitMargin
    let points = CGSize(width: image.width / scale, height: image.height / scale)
    let shrink = min(
      1, (visible.width - decor.width - margin) / points.width,
      (visible.height - decor.height - margin) / points.height)
    let size = CGSize(
      width: min(
        visible.width,
        max(minimum.width, (points.width * shrink).rounded(.up) + margin + decor.width)),
      height: min(
        visible.height,
        max(minimum.height, (points.height * shrink).rounded(.up) + margin + decor.height)))
    let x = min(max(frame.midX - size.width / 2, visible.minX), visible.maxX - size.width)
    let y = min(max(frame.midY - size.height / 2, visible.minY), visible.maxY - size.height)
    return CGRect(origin: CGPoint(x: x.rounded(), y: y.rounded()), size: size)
  }
  private func refresh() {
    window?.title = session.name
    window?.isDocumentEdited = session.dirty
    window?.representedURL = session.output ?? session.source
    empty.isHidden = session.image != nil || session.busy
    tools.isEnabled = !session.busy && !prompting && !interactionBlocked && session.state != nil
    canvas.enabled = regionActionsEnabled
    if observedGeneration != session.generation {
      observedGeneration = session.generation
      canvas.cancel()
    }
    if let state = session.state, shownRevision != state.revision {
      shownRevision = state.revision
      canvas.image = session.image
      canvas.imageSize = state.size
      canvas.clear()
      if shownSize != state.size {
        let first = shownSize == .zero
        shownSize = state.size
        canvas.fit()
        if first { fitWindow(to: state.size) }
        widthField?.stringValue = String(state.width)
        heightField?.stringValue = String(state.height)
      }
    }
    for view in options.arrangedSubviews {
      if let control = view as? NSControl { control.isEnabled = regionActionsEnabled }
    }
    refreshStatus()
    if showWhenLoaded, !session.busy {
      showWhenLoaded = false
      showWindow(nil)
    }
  }
  private func refreshStatus() {
    let ready = regionActionsEnabled
    for b in actionButtons {
      b.isEnabled = ready
      if b.action == #selector(extractSelection)
        || (b.action == #selector(applyTool) && tool == .mosaic)
      {
        b.isEnabled = ready && canvas.regions.selection != nil
      }
      if b.action == #selector(detectText) {
        b.isEnabled = ready && !session.detecting
        b.title = session.detecting ? "Detecting…" : "Detect text"
      }
      if b.action == #selector(removeBox) { b.isEnabled = ready && canvas.regions.active != nil }
      if b.action == #selector(applyTool), tool == .text {
        b.isEnabled = ready && !canvas.regions.boxes.isEmpty
      }
    }
    if session.busy || session.detecting {
      progress.startAnimation(nil)
    } else {
      progress.stopAnimation(nil)
    }
    dismiss.isHidden = session.error == nil
    status.textColor = session.error == nil ? .secondaryLabelColor : .systemRed
    if let error = session.error {
      status.stringValue = error
    } else if let state = session.state {
      var parts = ["\(state.width) × \(state.height) px", "\(canvas.zoomPercent)%"]
      if let r = canvas.regions.selected(tool) {
        parts.append("\(r.x), \(r.y) · \(r.width) × \(r.height) px")
      }
      if tool == .text { parts.append("\(canvas.regions.boxes.count) regions") }
      parts.append(session.message)
      status.stringValue = parts.joined(separator: "   ·   ")
    } else {
      status.stringValue = session.message
    }
    status.toolTip = status.stringValue
    canvas.setAccessibilityValue(status.stringValue)
  }
  func validateMenuItem(_ item: NSMenuItem) -> Bool {
    guard !prompting, !interactionBlocked else { return false }
    if item.action == #selector(chooseTool(_:)) {
      item.state = item.tag == tool.rawValue ? .on : .off
    }
    if item.action == #selector(undo(_:)) { return session.ready && session.state?.canUndo == true }
    if item.action == #selector(redo(_:)) { return session.ready && session.state?.canRedo == true }
    return session.ready
  }
}

extension EditorWindow: NSDraggingDestination {
  func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
    (prompting || interactionBlocked) ? [] : .copy
  }
  func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
    guard !prompting, !interactionBlocked,
      let urls = sender.draggingPasteboard.readObjects(
        forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], !urls.isEmpty
    else { return false }
    onOpen?(urls)
    return true
  }
}
