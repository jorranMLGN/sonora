import AVFoundation
import ExpoModulesCore
@_implementationOnly import SonoraRust

/// Takes an owned JSON string from the Rust core and releases it.
private func take(_ raw: UnsafeMutablePointer<CChar>?) -> String {
  guard let raw else { return "{\"error\":\"the core answered nothing\"}" }
  defer { sonora_free(raw) }
  return String(cString: raw)
}

/// The app's own Application Support folder, where the core keeps its files.
private func dataDir() -> String {
  let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("sonora", isDirectory: true)
  try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
  return base.path
}

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

    // These block on the network, so they run as async functions, off the JS thread.
    AsyncFunction("init") { () -> String in
      take(sonora_init(dataDir()))
    }

    AsyncFunction("search") { (query: String) -> String in
      take(sonora_search(query))
    }

    AsyncFunction("play") { (id: String) -> String in
      take(sonora_play(id))
    }

    Function("toggle") { () -> String in
      take(sonora_toggle())
    }

    Function("nowPlaying") { () -> String in
      take(sonora_now_playing())
    }
  }
}
