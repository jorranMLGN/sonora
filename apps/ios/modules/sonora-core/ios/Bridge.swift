import Foundation

/// Plain dictionaries for the records the Rust core hands Swift, in the shapes
/// `modules/sonora-core/index.ts` types them as. JS never sees a generated type.
enum Bridge {
  /// A Swift optional as JS sees it: the value, or null.
  static func optional<T>(_ value: T?) -> Any {
    value.map { $0 as Any } ?? NSNull()
  }

  static func item(_ item: Item) -> [String: Any] {
    [
      "id": item.id,
      "title": item.title,
      "subtitle": item.subtitle,
      "artwork": optional(item.artwork),
      "durationMs": Double(item.durationMs),
      "explicit": item.explicit,
      "provider": item.provider,
      "playable": item.playable,
    ]
  }

  static func items(_ items: [Item]) -> [[String: Any]] {
    items.map(item)
  }

  static func search(_ results: SearchResults) -> [String: Any] {
    [
      "tracks": items(results.tracks),
      "albums": items(results.albums),
      "artists": items(results.artists),
      "playlists": items(results.playlists),
    ]
  }

  static func status(_ status: Status) -> String {
    switch status {
    case .idle: return "idle"
    case .loading: return "loading"
    case .playing: return "playing"
    case .paused: return "paused"
    }
  }

  static func repeatMode(_ mode: Repeat) -> String {
    switch mode {
    case .off: return "off"
    case .all: return "all"
    case .one: return "one"
    }
  }

  static func state(_ state: PlayerState) -> [String: Any] {
    [
      "track": optional(state.track.map(item)),
      "status": status(state.status),
      "positionMs": Double(state.positionMs),
      "durationMs": Double(state.durationMs),
      "shuffle": state.shuffle,
      "repeat": repeatMode(state.repeat),
    ]
  }

  static func queue(_ queue: QueueSnapshot) -> [String: Any] {
    [
      "items": items(queue.items),
      "current": optional(queue.current.map { Int($0) }),
      "origin": optional(queue.origin),
      "shuffle": queue.shuffle,
      "repeat": repeatMode(queue.repeat),
    ]
  }

  static func session(_ state: SessionState) -> [String: Any] {
    switch state {
    case .signedOut: return ["kind": "signedOut"]
    case .connecting: return ["kind": "connecting"]
    case .guest: return ["kind": "guest"]
    case .signedIn(let account): return ["kind": "signedIn", "account": account]
    case .expired: return ["kind": "expired"]
    case .failed(let message): return ["kind": "failed", "message": message]
    }
  }

  static func provider(_ info: ProviderInfo) -> [String: Any] {
    [
      "slug": info.slug,
      "name": info.name,
      "signedIn": info.signedIn,
      "account": optional(info.account),
      "session": session(info.session),
    ]
  }

  static func prompt(_ prompt: Prompt) -> [String: Any] {
    switch prompt {
    case .url(let url): return ["kind": "url", "url": url]
    case .code(let code, let url): return ["kind": "code", "code": code, "url": url]
    case .secret: return ["kind": "secret"]
    case .accounts(let choices):
      return [
        "kind": "accounts",
        "choices": choices.map { ["id": $0.id, "name": $0.name, "detail": optional($0.detail)] },
      ]
    }
  }

  /// The words of a core failure, for a rejected promise.
  static func message(_ error: Error) -> String {
    if case CoreError.Failed(let message) = error {
      return message
    }
    return error.localizedDescription
  }
}

/// Forwards the core's events to JS. It holds the module weakly, so a reloaded bridge lets the
/// old module go.
final class Listener: EventListener, @unchecked Sendable {
  private weak var module: SonoraCoreModule?

  init(module: SonoraCoreModule) {
    self.module = module
  }

  func onState(state: PlayerState) {
    module?.sendEvent("onState", Bridge.state(state))
  }

  func onPosition(positionMs: UInt64) {
    module?.sendEvent("onPosition", ["positionMs": Double(positionMs)])
  }

  func onQueue(queue: QueueSnapshot) {
    module?.sendEvent("onQueue", Bridge.queue(queue))
  }

  func onSession(slug: String, state: SessionState) {
    module?.sendEvent("onSession", ["slug": slug, "state": Bridge.session(state)])
  }

  func onPrompt(slug: String, prompt: Prompt) {
    module?.sendEvent("onPrompt", ["slug": slug, "prompt": Bridge.prompt(prompt)])
  }

  func onError(message: String) {
    module?.sendEvent("onError", ["message": message])
  }
}

/// Starts the core. It lives out here because inside a class `init` names the initialiser,
/// not the generated function.
func startCore(dataDir: String, listener: EventListener) throws {
  try `init`(dataDir: dataDir, listener: listener)
}
