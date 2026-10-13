Pod::Spec.new do |s|
  s.name           = 'SonoraCore'
  s.version        = '0.0.1'
  s.summary        = 'Sonora Rust core for the iOS spike'
  s.description    = 'Calls the sonora mobile static library through a C ABI'
  s.author         = ''
  s.homepage       = 'https://github.com/jorranMLGN/sonora'
  s.license        = 'GPL-3.0-or-later'
  s.platforms      = { :ios => '16.4' }
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.source_files = '*.swift'
  s.preserve_paths = 'rust/**/*'
  s.vendored_libraries = 'rust/libmobile.a'
  s.frameworks = 'AudioToolbox', 'CoreAudio', 'AVFoundation', 'AVFAudio', 'CoreFoundation', 'Foundation'
  s.libraries = 'c++', 'resolv'

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_INCLUDE_PATHS' => '$(inherited) "$(PODS_TARGET_SRCROOT)/rust"',
  }
end
