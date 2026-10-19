import Foundation
import MediaPlayer
import UIKit

/// Publishes the current track to the lock screen and Control Center through
/// `MPNowPlayingInfoCenter`, and routes the remote commands to injected closures. It never
/// references the Rust bindings.
///
/// Create and use it on the main actor. The closures are called on the main actor too.
@MainActor
public final class NowPlaying {
  private let onPlay: () -> Void
  private let onPause: () -> Void
  private let onToggle: () -> Void
  private let onNext: () -> Void
  private let onPrevious: () -> Void
  private let onSeek: (Double) -> Void

  private var targets: [(MPRemoteCommand, Any)] = []
  private var info: [String: Any] = [:]
  private var artworkCache: [URL: MPMediaItemArtwork] = [:]
  private var currentArtworkURL: URL?
  private var artworkTask: Task<Void, Never>?

  /// Registers the remote commands. `onSeek` receives the target position in milliseconds.
  public init(
    onPlay: @escaping () -> Void,
    onPause: @escaping () -> Void,
    onToggle: @escaping () -> Void,
    onNext: @escaping () -> Void,
    onPrevious: @escaping () -> Void,
    onSeek: @escaping (Double) -> Void
  ) {
    self.onPlay = onPlay
    self.onPause = onPause
    self.onToggle = onToggle
    self.onNext = onNext
    self.onPrevious = onPrevious
    self.onSeek = onSeek
    registerCommands()
  }

  /// Publishes the track and playback position. Call it on every state change and every 5 s
  /// while playing. The artwork is fetched once per URL and applied when it arrives, if the
  /// track is still current.
  public func update(
    title: String,
    artist: String,
    album: String,
    durationMs: Double,
    elapsedMs: Double,
    playing: Bool,
    artworkURL: String?
  ) {
    let url = artworkURL.flatMap { URL(string: $0) }

    info[MPMediaItemPropertyTitle] = title
    info[MPMediaItemPropertyArtist] = artist
    info[MPMediaItemPropertyAlbumTitle] = album
    info[MPMediaItemPropertyPlaybackDuration] = max(0, durationMs) / 1000
    info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = max(0, elapsedMs) / 1000
    info[MPNowPlayingInfoPropertyPlaybackRate] = playing ? 1.0 : 0.0
    info[MPNowPlayingInfoPropertyDefaultPlaybackRate] = 1.0

    if url != currentArtworkURL {
      artworkTask?.cancel()
      artworkTask = nil
      currentArtworkURL = url
      info[MPMediaItemPropertyArtwork] = nil
    }
    if let url {
      if let cached = artworkCache[url] {
        info[MPMediaItemPropertyArtwork] = cached
      } else if artworkTask == nil {
        loadArtwork(url)
      }
    }

    publish()
    MPNowPlayingInfoCenter.default().playbackState = playing ? .playing : .paused
  }

  /// Removes the track from the lock screen.
  public func clear() {
    artworkTask?.cancel()
    artworkTask = nil
    currentArtworkURL = nil
    info = [:]
    MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    MPNowPlayingInfoCenter.default().playbackState = .stopped
  }

  /// Unregisters the remote commands and clears the lock screen. Call it when the module goes
  /// away.
  public func tearDown() {
    for (command, target) in targets {
      command.removeTarget(target)
    }
    targets = []
    clear()
  }

  private func publish() {
    MPNowPlayingInfoCenter.default().nowPlayingInfo = info
  }

  private func loadArtwork(_ url: URL) {
    artworkTask = Task { [weak self] in
      let data = try? await URLSession.shared.data(from: url).0
      guard !Task.isCancelled, let self else { return }
      self.applyArtwork(data: data, url: url)
    }
  }

  private func applyArtwork(data: Data?, url: URL) {
    artworkTask = nil
    guard let data, let image = UIImage(data: data) else { return }
    let artwork = Self.makeArtwork(image)
    artworkCache[url] = artwork
    guard url == currentArtworkURL else { return }
    info[MPMediaItemPropertyArtwork] = artwork
    publish()
  }

  /// Built outside the main actor: MediaPlayer calls the request handler on its own queue.
  nonisolated private static func makeArtwork(_ image: UIImage) -> MPMediaItemArtwork {
    MPMediaItemArtwork(boundsSize: image.size) { _ in image }
  }

  private func registerCommands() {
    let center = MPRemoteCommandCenter.shared()

    add(center.playCommand) { $0.onPlay() }
    add(center.pauseCommand) { $0.onPause() }
    add(center.togglePlayPauseCommand) { $0.onToggle() }
    add(center.nextTrackCommand) { $0.onNext() }
    add(center.previousTrackCommand) { $0.onPrevious() }

    let seekCommand = center.changePlaybackPositionCommand
    seekCommand.isEnabled = true
    let seek = seekCommand.addTarget { [weak self] event in
      guard let event = event as? MPChangePlaybackPositionCommandEvent else {
        return .commandFailed
      }
      let positionMs = event.positionTime * 1000
      Task { @MainActor [weak self] in
        self?.onSeek(positionMs)
      }
      return .success
    }
    targets.append((seekCommand, seek))
  }

  private func add(
    _ command: MPRemoteCommand, _ action: @escaping @MainActor (NowPlaying) -> Void
  ) {
    command.isEnabled = true
    let target = command.addTarget { [weak self] _ in
      Task { @MainActor [weak self] in
        guard let self else { return }
        action(self)
      }
      return .success
    }
    targets.append((command, target))
  }
}
