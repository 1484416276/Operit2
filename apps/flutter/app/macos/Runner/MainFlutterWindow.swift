import Cocoa
import desktop_multi_window
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  /// Creates the Flutter engine while retaining the window after close.
  override func awakeFromNib() {
    isReleasedWhenClosed = false
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.minSize = NSSize(width: 560, height: 420)
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    AppleRuntimeChannel.register(binaryMessenger: flutterViewController.engine.binaryMessenger)
    FlutterMultiWindowPlugin.setOnWindowCreatedCallback { childFlutterViewController in
      RegisterGeneratedPlugins(registry: childFlutterViewController)
      AppleRuntimeChannel.register(binaryMessenger: childFlutterViewController.engine.binaryMessenger)
    }

    super.awakeFromNib()
  }
}
