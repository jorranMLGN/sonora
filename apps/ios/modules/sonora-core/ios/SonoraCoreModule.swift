import AVFoundation
import ExpoModulesCore
@_implementationOnly import SonoraRust

public class SonoraCoreModule: Module {
  public func definition() -> ModuleDefinition {
    Name("SonoraCore")

    OnCreate {
      let session = AVAudioSession.sharedInstance()
      do {
        try session.setCategory(.playback, mode: .default)
        try session.setActive(true)
      } catch {
        NSLog("sonora: cannot activate the audio session: \(error)")
      }
    }

    Function("version") { () -> String in
      String(cString: sonora_version())
    }

    Function("playTone") { (seconds: Int) -> Int in
      Int(sonora_play_tone(UInt32(max(0, seconds))))
    }
  }
}
