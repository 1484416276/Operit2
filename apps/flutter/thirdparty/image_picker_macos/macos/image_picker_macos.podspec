Pod::Spec.new do |s|
  s.name = 'image_picker_macos'
  s.version = '0.0.1'
  s.summary = 'Native camera capture for Operit.'
  s.description = 'Presents the macOS camera panel and returns captured image paths.'
  s.homepage = 'https://github.com/AAswordman/Operit'
  s.license = { :type => 'BSD', :file => '../LICENSE' }
  s.author = 'Operit'
  s.source = { :path => '.' }
  s.source_files = 'Classes/**/*.swift'
  s.dependency 'FlutterMacOS'
  s.platform = :osx, '12.0'
  s.frameworks = 'AVFoundation', 'Quartz'
  s.swift_version = '5.0'
end
