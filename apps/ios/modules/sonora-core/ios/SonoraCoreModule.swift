import ExpoModulesCore

/// The app's own Application Support folder, where the core keeps its files.
private func dataDir() -> String {
  let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("sonora", isDirectory: true)
  try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
  return base.path
}

/// The Rust core as an Expo module. Every function hands an intent to the core; what happens
/// next arrives as one of the events. Generated functions are named through the module
/// (`SonoraCore.next()`), so they never resolve to something of `Module`'s.
public class SonoraCoreModule: Module {
  public func definition() -> ModuleDefinition {
    Name("SonoraCore")

    Events("onState", "onPosition", "onQueue", "onSession", "onPrompt", "onError")

    // The audio session, Now Playing and the remote commands live as long as the process.
    OnCreate {
      DispatchQueue.main.async {
        Platform.shared.start()
        PoTokenMinter.shared.start()
      }
    }

    // Registers this module as the listener; the core answers with the providers, the queue and
    // the player state at once.
    AsyncFunction("start") { () throws in
      try startCore(dataDir: dataDir(), listener: Listener(module: self))
    }

    // A few switches the app keeps for itself, such as whether the intro was seen. They live in
    // the app's user defaults, apart from the core's files.
    Function("flag") { (key: String) -> Bool in
      UserDefaults.standard.bool(forKey: "sonora.\(key)")
    }

    Function("setFlag") { (key: String, on: Bool) in
      UserDefaults.standard.set(on, forKey: "sonora.\(key)")
    }

    AsyncFunction("providers") { () throws -> [[String: Any]] in
      try SonoraCore.providers().map(Bridge.provider)
    }

    // Signing in. `signIn` settles when the sign-in does; a provider's refusal rejects with the
    // code `ERR_SIGN_IN_<PROBLEM>`, which JS words with the desktop's `login-problem-*` keys.
    AsyncFunction("signIn") { (slug: String, option: String) async throws in
      guard let chosen = Bridge.option(named: option) else {
        throw Exception(name: "UnknownOption", description: "no sign-in option is called \(option)")
      }
      do {
        try await SonoraCore.signIn(slug: slug, option: chosen)
      } catch CoreError.SignIn(let problem, let message) {
        throw Exception(
          name: "SignInFailed", description: message,
          code: "ERR_SIGN_IN_\(Bridge.problem(problem).uppercased())")
      }
    }

    AsyncFunction("answer") { (slug: String, input: String) throws in
      try SonoraCore.answer(slug: slug, input: input)
    }

    AsyncFunction("cancelSignIn") { (slug: String) throws in
      try SonoraCore.cancelSignIn(slug: slug)
    }

    AsyncFunction("signOut") { (slug: String) throws in
      try SonoraCore.signOut(slug: slug)
    }

    AsyncFunction("search") { (query: String) async throws -> [String: Any] in
      Bridge.search(try await SonoraCore.search(query: query))
    }

    AsyncFunction("play") { (ids: [String], index: Int, origin: String?) async throws in
      try await SonoraCore.play(ids: ids, index: UInt32(max(0, index)), origin: origin)
    }

    AsyncFunction("toggle") { () throws in
      try SonoraCore.toggle()
    }

    AsyncFunction("next") { () throws in
      try SonoraCore.next()
    }

    AsyncFunction("previous") { () throws in
      try SonoraCore.previous()
    }

    AsyncFunction("seek") { (positionMs: Double) throws in
      try SonoraCore.seek(positionMs: UInt64(max(0, positionMs)))
    }

    AsyncFunction("setShuffle") { (on: Bool) throws in
      try SonoraCore.setShuffle(on: on)
    }

    AsyncFunction("cycleRepeat") { () throws in
      try SonoraCore.cycleRepeat()
    }

    AsyncFunction("enqueueNext") { (ids: [String]) async throws in
      try await SonoraCore.enqueueNext(ids: ids)
    }

    AsyncFunction("enqueueLast") { (ids: [String]) async throws in
      try await SonoraCore.enqueueLast(ids: ids)
    }

    AsyncFunction("remove") { (index: Int) throws in
      try SonoraCore.remove(index: UInt32(max(0, index)))
    }

    // Home, the library and the album, playlist and artist pages.

    AsyncFunction("home") { () async throws -> [[String: Any]] in
      try await SonoraCore.home().map(Bridge.section)
    }

    AsyncFunction("library") { (slug: String, part: String, page: Int) async throws -> [String: Any] in
      guard let kind = Bridge.part(part) else { throw BadArgument(value: part) }
      let found = try await SonoraCore.library(slug: slug, part: kind, page: UInt32(max(0, page)))
      return Bridge.page(found)
    }

    AsyncFunction("cachedLibrary") { (slug: String, part: String) throws -> [String: Any]? in
      guard let kind = Bridge.part(part) else { throw BadArgument(value: part) }
      return try SonoraCore.cachedLibrary(slug: slug, part: kind).map(Bridge.page)
    }

    AsyncFunction("savedIds") { () async throws -> [String: Any] in
      Bridge.saved(try await SonoraCore.savedIds())
    }

    AsyncFunction("setSaved") { (id: String, kind: String, saved: Bool) async throws in
      guard let part = Bridge.part(kind) else { throw BadArgument(value: kind) }
      try await SonoraCore.setSaved(id: id, kind: part, saved: saved)
    }

    AsyncFunction("album") { (id: String) async throws -> [String: Any] in
      Bridge.album(try await SonoraCore.album(id: id))
    }

    AsyncFunction("playlist") { (id: String) async throws -> [String: Any] in
      Bridge.playlist(try await SonoraCore.playlist(id: id))
    }

    AsyncFunction("playlistMore") { (id: String, continuation: String) async throws -> [String: Any] in
      Bridge.playlistMore(try await SonoraCore.playlistMore(id: id, continuation: continuation))
    }

    AsyncFunction("artist") { (id: String) async throws -> [String: Any] in
      Bridge.artist(try await SonoraCore.artist(id: id))
    }

    // The url of a track's Canvas, the looping MP4 Spotify shows while it plays, or nil.
    AsyncFunction("canvas") { (id: String) async throws -> String? in
      try await SonoraCore.canvas(id: id)
    }

    // The Canvas video itself, muted and looping, which leaves the audio session alone.
    View(CanvasView.self) {
      Events("onReady", "onFailed")

      Prop("url") { (view: CanvasView, url: String?) in
        view.setUrl(url)
      }

      Prop("playing") { (view: CanvasView, playing: Bool?) in
        view.setPlaying(playing ?? true)
      }
    }

    AsyncFunction("playerState") { () throws -> [String: Any] in
      Bridge.state(try SonoraCore.playerState())
    }

    AsyncFunction("queue") { () throws -> [String: Any] in
      Bridge.queue(try SonoraCore.queue())
    }
  }
}
