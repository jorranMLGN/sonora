import AVFoundation
import ExpoModulesCore
import UIKit

/// A track's Canvas: a muted video that loops and fills its bounds, cropping what overflows.
///
/// It plays with its own `AVQueuePlayer` and never touches the audio session, so the music's
/// session, Now Playing and the remote commands stay as `Platform` set them. A player library
/// would switch the session to mix with others while a muted video plays, which takes the lock
/// screen controls away. `onReady` fires once the first frame can be drawn and `onFailed` when
/// the video cannot play; JS fades the view in and out on them.
public final class CanvasView: ExpoView {
  let onReady = EventDispatcher()
  let onFailed = EventDispatcher()

  private let player = AVQueuePlayer()
  private let playerLayer = AVPlayerLayer()
  private var looper: AVPlayerLooper?
  private var url: String?
  private var playing = true
  private var readyObservation: NSKeyValueObservation?
  private var failureObservation: NSKeyValueObservation?

  public required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
    player.isMuted = true
    player.preventsDisplaySleepDuringVideoPlayback = false
    player.audiovisualBackgroundPlaybackPolicy = .pauses
    playerLayer.player = player
    playerLayer.videoGravity = .resizeAspectFill
    layer.addSublayer(playerLayer)
    readyObservation = playerLayer.observe(\.isReadyForDisplay, options: [.new]) {
      [weak self] _, change in
      guard change.newValue == true else { return }
      Task { @MainActor [weak self] in
        self?.onReady()
      }
    }
  }

  public override func layoutSubviews() {
    super.layoutSubviews()
    // A bare sublayer animates a new frame by itself; the video should just be the right size.
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    playerLayer.frame = bounds
    CATransaction.commit()
  }

  /// Plays `address` from its start and loops it, or stops when it is nil.
  func setUrl(_ address: String?) {
    guard address != url else { return }
    url = address
    failureObservation = nil
    looper?.disableLooping()
    looper = nil
    player.removeAllItems()
    guard let address, let source = URL(string: address) else { return }
    let looper = AVPlayerLooper(player: player, templateItem: AVPlayerItem(url: source))
    failureObservation = looper.observe(\.status, options: [.new]) { [weak self] looper, _ in
      guard looper.status == .failed else { return }
      Task { @MainActor [weak self] in
        self?.onFailed()
      }
    }
    self.looper = looper
    if playing {
      player.play()
    }
  }

  /// Runs the video while the music plays and holds it on its frame while it is paused.
  func setPlaying(_ on: Bool) {
    playing = on
    if on {
      player.play()
    } else {
      player.pause()
    }
  }
}
