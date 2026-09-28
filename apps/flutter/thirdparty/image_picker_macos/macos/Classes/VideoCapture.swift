import AppKit
import AVFoundation
import FlutterMacOS

/// A user-controlled recording sheet. Session work is serialized off the UI thread.
final class PickerVideoCapture: NSObject, AVCaptureFileOutputRecordingDelegate, @unchecked Sendable {
  private let session = AVCaptureSession()
  private let movie = AVCaptureMovieFileOutput()
  private let queue = DispatchQueue(label: "image_picker_macos.video")
  private var panel: NSPanel?
  private var outputURL: URL?
  private var result: FlutterResult?
  private var cancelled = false
  private var recording = false
  private var finishing = false
  private var runtimeErrorObserver: NSObjectProtocol?
  private var button: NSButton?
  private var windowCloseObserver: NSObjectProtocol?
  private var keepOutput = false

  func start(args: [String: Any], result: @escaping FlutterResult) {
    self.result = result
    authorize(.video) { [weak self] granted in
      guard let self else { return }
      guard granted else { self.finish(error: self.error("camera_access_denied", "Camera permission is required")); return }
      self.authorize(.audio) { granted in
        guard granted else { self.finish(error: self.error("microphone_access_denied", "Microphone permission is required for video")); return }
        self.configure(args)
      }
    }
  }

  private func authorize(_ type: AVMediaType, completion: @escaping (Bool) -> Void) {
    switch AVCaptureDevice.authorizationStatus(for: type) {
    case .authorized: completion(true)
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: type) { granted in DispatchQueue.main.async { completion(granted) } }
    default: completion(false)
    }
  }

  private func configure(_ args: [String: Any]) {
    guard let parent = NSApp.keyWindow ?? NSApp.mainWindow, parent.attachedSheet == nil else {
      finish(error: error("camera_window_unavailable", "No available application window")); return
    }
    let preferred: AVCaptureDevice.Position = args["preferredCameraDevice"] as? String == "front" ? .front : .back
    let devices = AVCaptureDevice.DiscoverySession(deviceTypes: [.builtInWideAngleCamera, .externalUnknown],
      mediaType: .video, position: .unspecified).devices
    guard let camera = devices.first(where: { $0.position == preferred }) ?? AVCaptureDevice.default(for: .video),
          let microphone = AVCaptureDevice.default(for: .audio) else {
      finish(error: error("camera_unavailable", "A camera and microphone are required")); return
    }
    queue.async {
      do {
        self.session.beginConfiguration()
        defer { self.session.commitConfiguration() }
        for device in [camera, microphone] {
          let input = try AVCaptureDeviceInput(device: device)
          guard self.session.canAddInput(input) else { throw self.failure("Cannot attach capture device") }
          self.session.addInput(input)
        }
        guard self.session.canAddOutput(self.movie) else { throw self.failure("Cannot attach movie output") }
        self.session.addOutput(self.movie)
        if let seconds = (args["maxDurationSeconds"] as? NSNumber)?.doubleValue {
          guard seconds.isFinite, seconds > 0 else { throw self.failure("Invalid video duration") }
          self.movie.maxRecordedDuration = CMTime(seconds: seconds, preferredTimescale: 600)
        }
        DispatchQueue.main.async { self.present(parent: parent) }
      } catch {
        DispatchQueue.main.async { self.finish(error: self.error("video_setup_failed", error.localizedDescription)) }
      }
    }
  }

  private func present(parent: NSWindow) {
    let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 640, height: 440),
      styleMask: [.titled], backing: .buffered, defer: false)
    panel.title = NSLocalizedString("Record Video", comment: "Video capture title")
    let view = NSView(frame: NSRect(x: 0, y: 60, width: 640, height: 380))
    view.wantsLayer = true
    let preview = AVCaptureVideoPreviewLayer(session: session)
    preview.frame = view.bounds
    preview.videoGravity = .resizeAspect
    view.layer?.addSublayer(preview)
    panel.contentView?.addSubview(view)
    let record = NSButton(title: NSLocalizedString("Record", comment: "Start recording"), target: self, action: #selector(toggleRecording))
    record.frame = NSRect(x: 480, y: 14, width: 140, height: 32)
    record.bezelStyle = .rounded
    record.isEnabled = false
    panel.contentView?.addSubview(record)
    let cancel = NSButton(title: NSLocalizedString("Cancel", comment: "Cancel recording"), target: self, action: #selector(cancelRecording))
    cancel.keyEquivalent = "\u{1b}"
    cancel.frame = NSRect(x: 20, y: 14, width: 120, height: 32)
    cancel.bezelStyle = .rounded
    panel.contentView?.addSubview(cancel)
    self.panel = panel
    button = record
    runtimeErrorObserver = NotificationCenter.default.addObserver(forName: .AVCaptureSessionRuntimeError,
      object: session, queue: .main) { [weak self] notification in
        guard let self else { return }
        self.finish(error: self.error("video_capture_failed",
          (notification.userInfo?[AVCaptureSessionErrorKey] as? Error)?.localizedDescription ?? "Capture session failed"))
      }
    windowCloseObserver = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification,
      object: parent, queue: .main) { [weak self] _ in self?.cancelRecording() }
    parent.beginSheet(panel)
    queue.async {
      self.session.startRunning()
      DispatchQueue.main.async { if !self.finishing { record.isEnabled = true } }
    }
  }

  @objc private func toggleRecording() {
    guard !finishing else { return }
    button?.isEnabled = false
    if recording {
      movie.stopRecording()
    } else {
      recording = true
      let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".mov")
      outputURL = url
      movie.startRecording(to: url, recordingDelegate: self)
    }
  }

  @objc private func cancelRecording() {
    cancelled = true
    if recording { movie.stopRecording() } else { finish() }
  }

  func fileOutput(_ output: AVCaptureFileOutput, didStartRecordingTo fileURL: URL,
                  from connections: [AVCaptureConnection]) {
    DispatchQueue.main.async {
      if self.cancelled { self.movie.stopRecording(); return }
      self.button?.title = NSLocalizedString("Finish", comment: "Finish recording")
      self.button?.isEnabled = true
    }
  }

  func fileOutput(_ output: AVCaptureFileOutput, didFinishRecordingTo fileURL: URL,
                  from connections: [AVCaptureConnection], error: Error?) {
    DispatchQueue.main.async {
      if self.finishing {
        if !self.keepOutput { try? FileManager.default.removeItem(at: fileURL) }
        return
      }
      let nsError = error as NSError?
      let succeeded = nsError == nil || nsError?.userInfo[AVErrorRecordingSuccessfullyFinishedKey] as? Bool == true
      if self.cancelled { self.finish(); return }
      if !succeeded { self.finish(error: self.error("video_capture_failed", error!.localizedDescription)); return }
      let size = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
      guard size > 0 else { self.finish(error: self.error("video_capture_failed", "Recording is empty")); return }
      self.finish(path: fileURL.path)
    }
  }

  private func finish(path: String? = nil, error: FlutterError? = nil) {
    guard !finishing else { return }
    finishing = true
    keepOutput = path != nil
    if let observer = windowCloseObserver { NotificationCenter.default.removeObserver(observer) }
    if let observer = runtimeErrorObserver { NotificationCenter.default.removeObserver(observer) }
    if let panel { panel.sheetParent?.endSheet(panel); panel.orderOut(nil) }
    panel = nil
    queue.async {
      self.session.stopRunning()
      if path == nil, let url = self.outputURL { try? FileManager.default.removeItem(at: url) }
      DispatchQueue.main.async {
        let result = self.result
        self.result = nil
        if let error { result?(error) } else { result?(path) }
      }
    }
  }

  private func error(_ code: String, _ message: String) -> FlutterError { FlutterError(code: code, message: message, details: nil) }
  private func failure(_ message: String) -> NSError { NSError(domain: "ImagePickerMacOS", code: 1, userInfo: [NSLocalizedDescriptionKey: message]) }
}
