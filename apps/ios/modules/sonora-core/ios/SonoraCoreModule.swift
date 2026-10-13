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

    AsyncFunction("playerState") { () throws -> [String: Any] in
      Bridge.state(try SonoraCore.playerState())
    }

    AsyncFunction("queue") { () throws -> [String: Any] in
      Bridge.queue(try SonoraCore.queue())
    }
  }
}
