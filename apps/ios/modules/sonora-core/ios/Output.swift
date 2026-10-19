import AVFoundation
import AVKit
import ExpoModulesCore

/// Where the sound goes: the system's output picker as a view, the current output, and an event
/// when it changes. It is a module of its own so the player can pick AirPlay speakers, TVs and
/// Bluetooth devices without the core knowing; RemoteIO, under the core's output, follows
/// whatever route the system picks.
public class SonoraRouteModule: Module {
  public func definition() -> ModuleDefinition {
    Name("SonoraRoute")

    Events("onRoute")

    OnStartObserving("onRoute") {
      NotificationCenter.default.addObserver(
        self, selector: #selector(routeChanged), name: AVAudioSession.routeChangeNotification,
        object: nil)
    }

    OnStopObserving("onRoute") {
      // swiftlint:disable:next notification_center_detachment
      NotificationCenter.default.removeObserver(self)
    }

    // The output playing now, as `{ name, kind }`.
    Function("outputRoute") { () -> [String: Any] in
      OutputRoute.current()
    }

    View(RoutePickerView.self) {
      Prop("tint") { (view, color: UIColor?) in
        view.setTint(color)
      }

      Prop("activeTint") { (view, color: UIColor?) in
        view.setActiveTint(color)
      }
    }
  }

  @objc func routeChanged(_ notification: Notification) {
    sendEvent("onRoute", OutputRoute.current())
  }
}

/// The current output of the audio session, in the shape `modules/sonora-core/route.ts` types.
enum OutputRoute {
  static func current() -> [String: Any] {
    guard let output = AVAudioSession.sharedInstance().currentRoute.outputs.first else {
      return ["name": "", "kind": "phone"]
    }
    return ["name": output.portName, "kind": kind(output.portType)]
  }

  /// The kinds of output JS tells apart; the phone's own speaker and earpiece are `phone`.
  private static func kind(_ port: AVAudioSession.Port) -> String {
    switch port {
    case .builtInSpeaker, .builtInReceiver:
      return "phone"
    case .airPlay:
      return "airplay"
    case .bluetoothA2DP, .bluetoothLE, .bluetoothHFP:
      return "bluetooth"
    case .carAudio:
      return "car"
    case .headphones, .headsetMic, .lineOut, .usbAudio, .HDMI:
      return "wired"
    default:
      return "other"
    }
  }
}

/// The system's output picker button. Tapping it lists AirPlay speakers, TVs and Bluetooth
/// devices; while it is open, a route change is the person's choice, not a lost device.
final class RoutePickerView: ExpoView {
  private let picker = AVRoutePickerView()

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    picker.prioritizesVideoDevices = false
    picker.delegate = self
    addSubview(picker)
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    picker.frame = bounds
  }

  /// The button's colour while the phone plays by itself; nil puts the system's back.
  func setTint(_ color: UIColor?) {
    picker.tintColor = color
  }

  /// The button's colour while another device plays; nil puts the system's back.
  func setActiveTint(_ color: UIColor?) {
    picker.activeTintColor = color
  }
}

extension RoutePickerView: @preconcurrency AVRoutePickerViewDelegate {
  func routePickerViewWillBeginPresentingRoutes(_ routePickerView: AVRoutePickerView) {
    AudioSession.choosingRoute(true)
  }

  func routePickerViewDidEndPresentingRoutes(_ routePickerView: AVRoutePickerView) {
    AudioSession.choosingRoute(false)
  }
}
