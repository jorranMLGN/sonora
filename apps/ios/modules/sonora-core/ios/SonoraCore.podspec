Pod::Spec.new do |s|
  s.name           = 'SonoraCore'
  s.version        = '0.0.1'
  s.summary        = 'The Sonora Rust core for iOS'
  s.description    = 'Calls the sonora mobile library through its generated UniFFI bindings'
  s.author         = ''
  s.homepage       = 'https://github.com/jorranMLGN/sonora'
  s.license        = 'GPL-3.0-or-later'
  s.platforms      = { :ios => '16.4' }
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  # `generated/` holds what uniffi-bindgen writes (mobile.swift, mobileFFI.h) and
  # `rust/SonoraRust.xcframework` the static library; CI puts both in place before `pod install`.
  s.source_files = '*.swift', 'generated/*.{swift,h}'
  s.public_header_files = 'generated/*.h'
  s.preserve_paths = 'rust/**/*'
  s.vendored_frameworks = 'rust/SonoraRust.xcframework'
  s.frameworks = 'AudioToolbox', 'CoreAudio', 'AVFoundation', 'AVFAudio', 'AVKit', 'CoreFoundation',
                 'Foundation', 'MediaPlayer', 'SafariServices', 'Security', 'SystemConfiguration', 'UIKit', 'WebKit'
  s.libraries = 'c++', 'resolv', 'iconv', 'objc'

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
  }
end
