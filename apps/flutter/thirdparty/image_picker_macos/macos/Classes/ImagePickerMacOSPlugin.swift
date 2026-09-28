import AVFoundation
import AppKit
import Quartz
import FlutterMacOS

public final class ImagePickerMacOSPlugin: NSObject, FlutterPlugin {
  // Camera panels are process-wide, including apps with multiple Flutter engines.
  private static let camera = AppleCameraCapture()
  private static var video: PickerVideoCapture?
  private static var captureBusy = false

  public static func register(with registrar: FlutterPluginRegistrar) {
    let channel = FlutterMethodChannel(name: "plugins.flutter.io/image_picker_macos", binaryMessenger: registrar.messenger)
    registrar.addMethodCallDelegate(ImagePickerMacOSPlugin(), channel: channel)
  }

  public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    if call.method == "processImage" {
      DispatchQueue.global(qos: .userInitiated).async {
        do {
          let path = try PickerImageProcessing.process(args)
          DispatchQueue.main.async { result(path) }
        } catch {
          DispatchQueue.main.async { result(FlutterError(code: "image_processing_failed", message: error.localizedDescription, details: nil)) }
        }
      }
      return
    }
    guard call.method == "takePhoto" || call.method == "recordVideo" else {
      result(FlutterMethodNotImplemented)
      return
    }
    guard !Self.captureBusy else {
      result(FlutterError(code: "camera_busy", message: "A camera request is already active", details: nil))
      return
    }
    Self.captureBusy = true
    let completion: FlutterResult = { value in
      Self.captureBusy = false
      Self.video = nil
      result(value)
    }
    if call.method == "takePhoto" {
      Self.camera.capture(result: completion)
    } else {
      let video = PickerVideoCapture()
      Self.video = video
      video.start(args: args, result: completion)
    }
  }
}

/// Presents the system camera panel and returns a local attachment image.
private final class AppleCameraCapture: NSObject {
  private var pendingResult: FlutterResult?
  private var pictureTaker: IKPictureTaker?

  func capture(result: @escaping FlutterResult) {
    guard pendingResult == nil else {
      result(FlutterError(code: "camera_busy", message: "A camera request is already active", details: nil))
      return
    }
    pendingResult = result
    switch AVCaptureDevice.authorizationStatus(for: .video) {
    case .authorized:
      present()
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
        DispatchQueue.main.async {
          guard let self else { return }
          if granted { self.present() } else { self.permissionDenied() }
        }
      }
    default:
      permissionDenied()
    }
  }

  private func permissionDenied() {
    finish(FlutterError(code: "camera_access_denied",
      message: "Camera access is disabled. Enable Operit2 in System Settings > Privacy & Security > Camera.", details: nil))
  }

  private func present() {
    guard AVCaptureDevice.default(for: .video) != nil else {
      finish(FlutterError(code: "camera_unavailable", message: "No camera is available on this Mac", details: nil))
      return
    }
    guard let window = NSApp.keyWindow ?? NSApp.mainWindow,
          window.isVisible, window.attachedSheet == nil else {
      finish(FlutterError(code: "camera_window_unavailable", message: "Return to the chat window before taking a photo", details: nil))
      return
    }
    let picker = IKPictureTaker()
    pictureTaker = picker
    picker.setValue(true, forKey: IKPictureTakerAllowsVideoCaptureKey)
    picker.setValue(false, forKey: IKPictureTakerAllowsFileChoosingKey)
    picker.setValue(false, forKey: IKPictureTakerShowRecentPictureKey)
    picker.setValue(false, forKey: IKPictureTakerUpdateRecentPictureKey)
    picker.setValue(false, forKey: IKPictureTakerShowAddressBookPictureKey)
    picker.setValue(false, forKey: IKPictureTakerAllowsEditingKey)
    picker.beginSheet(for: window, withDelegate: self,
      didEnd: #selector(pictureTakerDidEnd(_:returnCode:contextInfo:)), contextInfo: nil)
  }

  @objc private func pictureTakerDidEnd(_ picker: IKPictureTaker,
                                       returnCode: Int,
                                       contextInfo: UnsafeMutableRawPointer?) {
    guard returnCode == NSApplication.ModalResponse.OK.rawValue else {
      finish(nil)
      return
    }
    guard let tiff = picker.outputImage()?.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let data = bitmap.representation(using: .jpeg, properties: [.compressionFactor: 1.0]) else {
      finish(FlutterError(code: "camera_image_error", message: "Could not encode the captured photo", details: nil))
      return
    }
    do {
      let directory = FileManager.default.temporaryDirectory.appendingPathComponent("camera", isDirectory: true)
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      let output = directory.appendingPathComponent("\(UUID().uuidString).jpg")
      try data.write(to: output, options: .atomic)
      finish(output.path)
    } catch {
      finish(FlutterError(code: "camera_save_error", message: error.localizedDescription, details: nil))
    }
  }

  private func finish(_ value: Any?) {
    let result = pendingResult
    pendingResult = nil
    pictureTaker?.orderOut(nil)
    pictureTaker = nil
    result?(value)
  }
}
