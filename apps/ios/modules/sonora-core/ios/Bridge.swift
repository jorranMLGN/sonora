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
      "album": item.album,
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
    case .failed(let message, let problem):
      return ["kind": "failed", "message": message, "problem": optional(problem.map(Bridge.problem))]
    }
  }

  static func provider(_ info: ProviderInfo) -> [String: Any] {
    [
      "slug": info.slug,
      "name": info.name,
      "signedIn": info.signedIn,
      "account": optional(info.account),
      "session": session(info.session),
      "options": info.options.map(option),
    ]
  }

  static func option(_ option: SignInOption) -> String {
    switch option {
    case .account: return "account"
    case .guest: return "guest"
    case .web: return "web"
    case .paste: return "paste"
    }
  }

  /// The option JS names, or nil for a name the core does not know.
  static func option(named name: String) -> SignInOption? {
    switch name {
    case "account": return .account
    case "guest": return .guest
    case "web": return .web
    case "paste": return .paste
    default: return nil
    }
  }

  /// A sign-in problem as JS sees it, the suffix of the desktop's `login-problem-*` keys.
  static func problem(_ problem: Problem) -> String {
    switch problem {
    case .premium: return "premium"
    case .region: return "region"
    case .credentials: return "credentials"
    case .secret: return "secret"
    case .network: return "network"
    case .cancelled: return "cancelled"
    case .refused: return "refused"
    }
  }

  static func web(_ web: WebSignIn) -> [String: Any] {
    [
      "url": web.url,
      "landing": web.landing,
      "domain": web.domain,
      "proof": web.proof,
      "agent": optional(web.agent),
    ]
  }

  static func prompt(_ prompt: Prompt) -> [String: Any] {
    switch prompt {
    case .url(let url): return ["kind": "url", "url": url]
    case .code(let code, let url): return ["kind": "code", "code": code, "url": url]
    case .secret(let web): return ["kind": "secret", "web": optional(web.map(Bridge.web))]
    case .accounts(let choices):
      return [
        "kind": "accounts",
        "choices": choices.map { ["id": $0.id, "name": $0.name, "detail": optional($0.detail)] },
      ]
    }
  }

  /// The library part JS names (`tracks`, `albums`, `artists`, `playlists`), or nil for any
  /// other word.
  static func part(_ name: String) -> Part? {
    switch name {
    case "tracks": return .songs
    case "albums": return .albums
    case "artists": return .artists
    case "playlists": return .playlists
    default: return nil
    }
  }

  static func page(_ page: LibraryPage) -> [String: Any] {
    [
      "items": items(page.items),
      "total": Int(page.total),
      "hasMore": page.hasMore,
    ]
  }

  static func sectionKind(_ kind: SectionKind) -> String {
    switch kind {
    case .listenAgain: return "listenAgain"
    case .quickPicks: return "quickPicks"
    case .shelf: return "shelf"
    }
  }

  static func section(_ section: HomeSection) -> [String: Any] {
    [
      "id": section.id,
      "title": section.title,
      "provider": section.provider,
      "kind": sectionKind(section.kind),
      "tracks": items(section.tracks),
      "albums": items(section.albums),
      "artists": items(section.artists),
      "playlists": items(section.playlists),
    ]
  }

  static func album(_ detail: AlbumDetail) -> [String: Any] {
    ["album": item(detail.album), "tracks": items(detail.tracks)]
  }

  static func playlist(_ detail: PlaylistDetail) -> [String: Any] {
    [
      "playlist": item(detail.playlist),
      "tracks": items(detail.tracks),
      "continuation": optional(detail.continuation),
    ]
  }

  static func playlistMore(_ more: PlaylistMore) -> [String: Any] {
    ["tracks": items(more.tracks), "continuation": optional(more.continuation)]
  }

  static func artist(_ detail: ArtistDetail) -> [String: Any] {
    [
      "artist": item(detail.artist),
      "topTracks": items(detail.topTracks),
      "albums": items(detail.albums),
    ]
  }

  static func saved(_ ids: SavedIds) -> [String: Any] {
    [
      "tracks": ids.tracks,
      "albums": ids.albums,
      "artists": ids.artists,
      "playlists": ids.playlists,
    ]
  }

  /// The words of a core failure, for a rejected promise.
  static func message(_ error: Error) -> String {
    switch error {
    case CoreError.Failed(let message): return message
    case CoreError.SignIn(_, let message): return message
    default: break
    }
    return error.localizedDescription
  }
}

/// A JS argument the core has no value for, such as an unknown library part.
struct BadArgument: LocalizedError {
  let value: String

  var errorDescription: String? { "unknown value \(value)" }
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
    DispatchQueue.main.async { Platform.shared.state(state) }
  }

  func onPosition(positionMs: UInt64) {
    module?.sendEvent("onPosition", ["positionMs": Double(positionMs)])
    DispatchQueue.main.async { Platform.shared.position(positionMs) }
  }

  func onQueue(queue: QueueSnapshot) {
    module?.sendEvent("onQueue", Bridge.queue(queue))
  }

  func onSession(slug: String, state: SessionState) {
    module?.sendEvent("onSession", ["slug": slug, "state": Bridge.session(state)])
    DispatchQueue.main.async { SignInPresenter.shared.session(slug, state) }
  }

  /// Every prompt goes to JS, which shows what is asked; the ones that need a browser are also
  /// answered here.
  func onPrompt(slug: String, prompt: Prompt) {
    module?.sendEvent("onPrompt", ["slug": slug, "prompt": Bridge.prompt(prompt)])
    DispatchQueue.main.async { SignInPresenter.shared.prompt(slug, prompt) }
  }

  func onError(message: String) {
    module?.sendEvent("onError", ["message": message])
  }

  /// A warning or error from the core's log, for the JS log only.
  func onLog(level: String, target: String, message: String) {
    module?.sendEvent("onLog", ["level": level, "target": target, "message": message])
  }
}

/// Starts the core. It lives out here because inside a class `init` names the initialiser,
/// not the generated function.
func startCore(dataDir: String, listener: EventListener) throws {
  try `init`(dataDir: dataDir, listener: listener)
}
