import Foundation

/// Ties the audio session and Now Playing to the Rust core: system interruptions, lost routes
/// and remote commands go to the core, and the core's player state goes to the lock screen.
/// One per process, started once from the module.
@MainActor
final class Platform {
  static let shared = Platform()

  /// How often Now Playing is published again while playing.
  private static let republish: UInt64 = 5_000_000_000

  private var audioSession: AudioSession?
  private var nowPlaying: NowPlaying?
  private var latest: PlayerState?
  private var positionMs: UInt64 = 0
  private var ticker: Task<Void, Never>?

  /// Activates the audio session, starts observing it and registers the remote commands.
  /// Calling it again does nothing.
  func start() {
    guard audioSession == nil else { return }
    audioSession = AudioSession(
      onInterrupted: { report(try SonoraCore.audioInterrupted()) },
      onResumed: { shouldResume in
        report(try SonoraCore.audioResumed(shouldResume: shouldResume))
      },
      onRouteLost: { report(try SonoraCore.routeChanged()) }
    )
    nowPlaying = NowPlaying(
      onPlay: { report(try SonoraCore.resume()) },
      onPause: { report(try SonoraCore.pause()) },
      onToggle: { report(try SonoraCore.toggle()) },
      onNext: { report(try SonoraCore.next()) },
      onPrevious: { report(try SonoraCore.previous()) },
      onSeek: { ms in report(try SonoraCore.seek(positionMs: UInt64(max(0, ms)))) }
    )
  }

  /// Takes every player state the core reports, publishes it, and keeps publishing every 5 s
  /// while it plays.
  func state(_ state: PlayerState) {
    latest = state
    positionMs = state.positionMs
    publish()
    ticker?.cancel()
    ticker = nil
    guard state.status == .playing else { return }
    ticker = Task { @MainActor [weak self] in
      while !Task.isCancelled {
        try? await Task.sleep(nanoseconds: Platform.republish)
        if Task.isCancelled { return }
        self?.publish()
      }
    }
  }

  /// Remembers the position; it reaches the lock screen with the next publish.
  func position(_ ms: UInt64) {
    positionMs = ms
  }

  private func publish() {
    guard let state = latest, let track = state.track else {
      nowPlaying?.clear()
      return
    }
    nowPlaying?.update(
      title: track.title,
      artist: track.subtitle,
      album: track.album,
      durationMs: Double(state.durationMs),
      elapsedMs: Double(positionMs),
      playing: state.status == .playing,
      artworkURL: track.artwork
    )
  }
}

/// Runs a core call for a system event, which has nobody to hand a failure to but the log.
private func report(_ call: @autoclosure () throws -> Void) {
  do {
    try call()
  } catch {
    NSLog("sonora: a call from the system failed: \(Bridge.message(error))")
  }
}
