import AVFoundation
import Foundation

/// Owns the app's `AVAudioSession`: configures it for playback, activates it and reports
/// interruptions and route changes through injected closures. It never references the Rust
/// bindings, the module wires the closures to the bridge.
///
/// Create and use it on the main actor. The closures are called on the main actor too.
@MainActor
public final class AudioSession {
  /// An interruption (call, alarm, Siri) began. The core should pause.
  private let onInterrupted: () -> Void
  /// An interruption ended; the flag is true when the system says playback may resume.
  private let onResumed: (Bool) -> Void
  /// The old output device went away (headphones unplugged) or the media services were reset.
  /// The core should pause and open the output again on the next play.
  private let onRouteLost: () -> Void

  private var observers: [NSObjectProtocol] = []

  /// How long after the app's output picker closes a route change still counts as chosen there.
  private static let choiceWindow: TimeInterval = 5
  /// When the app's output picker last closed, or `.distantFuture` while it is open.
  private static var choiceAt: Date = .distantPast

  /// Configures and activates the session, then starts observing. Failure to activate is logged,
  /// not thrown: observers stay registered so a later interruption end can recover.
  public init(
    onInterrupted: @escaping () -> Void,
    onResumed: @escaping (Bool) -> Void,
    onRouteLost: @escaping () -> Void
  ) {
    self.onInterrupted = onInterrupted
    self.onResumed = onResumed
    self.onRouteLost = onRouteLost
    configure()
    observe()
  }

  /// Sets the playback category and activates the session. Safe to call again, for instance
  /// after the media services were reset or an interruption ended. `.playback` may already go
  /// to AirPlay and Bluetooth; `.allowAirPlay` is only for `.playAndRecord`, and the long-form
  /// route policy wants an `AVPlayer`, which the core's RemoteIO output is not.
  public func configure() {
    let session = AVAudioSession.sharedInstance()
    do {
      try session.setCategory(.playback, mode: .default)
      try session.setActive(true)
    } catch {
      NSLog("sonora: cannot activate the audio session: \(error)")
    }
  }

  /// Marks the app's output picker as open or closed, so a route the person picks there is not
  /// taken for a lost device.
  public static func choosingRoute(_ open: Bool) {
    choiceAt = open ? .distantFuture : Date()
  }

  /// Stops observing. Call it when the module goes away.
  public func tearDown() {
    for observer in observers {
      NotificationCenter.default.removeObserver(observer)
    }
    observers = []
  }

  private func observe() {
    let center = NotificationCenter.default
    let session = AVAudioSession.sharedInstance()

    observers.append(
      center.addObserver(
        forName: AVAudioSession.interruptionNotification, object: session, queue: .main
      ) { [weak self] note in
        let info = note.userInfo
        let rawType = info?[AVAudioSessionInterruptionTypeKey] as? UInt
        let rawOptions = info?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0
        Task { @MainActor [weak self] in
          self?.handleInterruption(rawType: rawType, rawOptions: rawOptions)
        }
      })

    observers.append(
      center.addObserver(
        forName: AVAudioSession.routeChangeNotification, object: session, queue: .main
      ) { [weak self] note in
        let rawReason = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt
        Task { @MainActor [weak self] in
          self?.handleRouteChange(rawReason: rawReason)
        }
      })

    observers.append(
      center.addObserver(
        forName: AVAudioSession.mediaServicesWereResetNotification, object: session,
        queue: .main
      ) { [weak self] _ in
        Task { @MainActor [weak self] in
          self?.configure()
          self?.onRouteLost()
        }
      })
  }

  private func handleInterruption(rawType: UInt?, rawOptions: UInt) {
    guard let rawType, let type = AVAudioSession.InterruptionType(rawValue: rawType) else {
      return
    }
    switch type {
    case .began:
      onInterrupted()
    case .ended:
      let options = AVAudioSession.InterruptionOptions(rawValue: rawOptions)
      let shouldResume = options.contains(.shouldResume)
      if shouldResume {
        configure()
      }
      onResumed(shouldResume)
    @unknown default:
      break
    }
  }

  private func handleRouteChange(rawReason: UInt?) {
    guard let rawReason, let reason = AVAudioSession.RouteChangeReason(rawValue: rawReason)
    else {
      return
    }
    switch reason {
    case .oldDeviceUnavailable:
      // Only a fall back to the phone's own speaker pauses, so music never blares out of it
      // when headphones come out or an AirPlay speaker drops. A move to AirPlay or Bluetooth,
      // or to the phone through the app's picker, is a choice that RemoteIO follows by itself.
      let chosen = Date().timeIntervalSince(AudioSession.choiceAt) <= AudioSession.choiceWindow
      guard AudioSession.onPhone(), !chosen else { return }
      onRouteLost()
    default:
      // A new device (headphones in, an AirPlay speaker picked) takes over by itself; playback
      // goes on as it was.
      break
    }
  }

  /// Whether every output of the current route is the phone's own speaker or earpiece.
  private static func onPhone() -> Bool {
    AVAudioSession.sharedInstance().currentRoute.outputs.allSatisfy {
      $0.portType == .builtInSpeaker || $0.portType == .builtInReceiver
    }
  }
}
