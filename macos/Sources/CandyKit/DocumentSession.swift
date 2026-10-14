import AppKit

@MainActor
final class DocumentSession {
  let id = UUID()
  private let worker = DispatchQueue(label: "me.told.candy.app.document", qos: .userInitiated)
  private var engine: RustDocument?
  private(set) var state: DocumentState?
  private(set) var image: CGImage?
  private(set) var source: URL?
  private(set) var output: URL?
  private(set) var saveSettings = SaveSettings()
  private(set) var busy = false
  private(set) var detecting = false
  private(set) var alive = true
  private(set) var generation: UInt64 = 0
  var name = "Candy"
  var message = localized("Ready")
  var error: String?
  var onChange: (() -> Void)?
  var onRegions: (([PixelRect]) -> Void)?
  var dirty: Bool { state?.dirty ?? false }
  var ready: Bool { alive && engine != nil && !busy }

  func invalidate() {
    alive = false
    generation += 1
    onChange = nil
    onRegions = nil
  }
  func fail(_ failure: Error) {
    error = failure.localizedDescription
    onChange?()
  }
  func clearError() {
    error = nil
    onChange?()
  }
  private func receive(_ rendered: RenderedDocument) {
    state = rendered.state
    image = rendered.preview.image()
  }
  func load(_ url: URL) {
    guard !busy, engine == nil else { return }
    busy = true
    source = url
    name = url.lastPathComponent
    message = localized("Opening image…")
    onChange?()
    worker.async { [self] in
      let result = Result {
        let engine = try RustDocument.open(url)
        return try (engine, engine.render())
      }
      DispatchQueue.main.async { [self] in
        guard alive else { return }
        busy = false
        switch result {
        case .success((let document, let rendered)):
          engine = document
          receive(rendered)
          message = localized("Image opened")
        case .failure(let failure): error = failure.localizedDescription
        }
        onChange?()
      }
    }
  }
  func adopt(_ document: RustDocument, rendered: RenderedDocument, name: String) {
    engine = document
    self.name = name
    receive(rendered)
    message = localized("Selection opened as a new image")
    onChange?()
  }
  private func perform<T: Sendable>(
    _ work: @escaping @Sendable (RustDocument) throws -> T,
    completion: @escaping @MainActor @Sendable (Result<T, Error>) -> Void
  ) {
    guard let engine else { return }
    worker.async {
      let result = Result { try work(engine) }
      DispatchQueue.main.async { completion(result) }
    }
  }
  func apply(_ edit: Edit) {
    guard ready else { return }
    busy = true
    generation += 1
    error = nil
    message = localized("Applying edit…")
    onChange?()
    perform({ d in
      try d.apply(edit)
      return try d.render()
    }) { [weak self] result in
      guard let self, self.alive else { return }
      self.busy = false
      switch result {
      case .success(let rendered):
        self.receive(rendered)
        self.message = localized("Edit applied")
      case .failure(let error): self.error = error.localizedDescription
      }
      self.onChange?()
    }
  }
  func extract(
    _ rect: PixelRect,
    completion: @escaping @MainActor @Sendable (RustDocument, RenderedDocument, String) -> Void
  ) {
    guard ready else { return }
    busy = true
    message = localized("Extracting selection…")
    onChange?()
    let name = (self.name as NSString).deletingPathExtension + "-crop.png"
    perform({ d in
      let new = try d.extract(rect)
      return try (new, new.render())
    }) { [weak self] result in
      guard let self, self.alive else { return }
      self.busy = false
      switch result {
      case .success((let document, let rendered)):
        self.message = localized("Selection extracted")
        completion(document, rendered, name)
      case .failure(let error): self.error = error.localizedDescription
      }
      self.onChange?()
    }
  }
  func detect() {
    guard ready, !detecting else { return }
    detecting = true
    error = nil
    message = localized("Detecting text…")
    onChange?()
    let token = generation
    perform({ try $0.snapshot() }) { [weak self] result in
      guard let self, self.alive else { return }
      switch result {
      case .failure(let error):
        self.detecting = false
        self.fail(error)
      case .success(let image):
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
          let regions = Result { try image.detect() }
          DispatchQueue.main.async { [weak self] in
            self?.acceptDetection(regions, generation: token)
          }
        }
      }
    }
  }
  func acceptDetection(_ result: Result<[PixelRect], Error>, generation token: UInt64) {
    guard alive else { return }
    detecting = false
    guard token == generation else {
      message = localized("Image changed. Detect text again.")
      onChange?()
      return
    }
    switch result {
    case .success(let regions):
      onRegions?(regions)
      message = localizedFormat("%ld text regions. Review before applying.", regions.count)
    case .failure(let error): self.error = error.localizedDescription
    }
    onChange?()
  }
  func save(
    _ url: URL, settings: SaveSettings? = nil,
    completion: @escaping @MainActor @Sendable (Bool) -> Void
  ) {
    guard ready else {
      completion(false)
      return
    }
    busy = true
    let settings = settings ?? saveSettings
    let options = settings.options(for: ExportFormat.forFilename(url.lastPathComponent))
    error = nil
    message = localized("Saving image…")
    onChange?()
    perform({ try $0.save(url, options: options) }) { [weak self] result in
      guard let self, self.alive else {
        completion(false)
        return
      }
      self.busy = false
      switch result {
      case .success(let state):
        self.state = state
        self.output = url
        self.saveSettings = settings
        self.name = url.lastPathComponent
        self.message = localizedFormat("Saved %@", self.name)
        self.onChange?()
        completion(true)
      case .failure(let error):
        self.error = error.localizedDescription
        self.onChange?()
        completion(false)
      }
    }
  }
}
