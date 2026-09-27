// Copyright 2013 The Flutter Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import Flutter
import ObjectiveC
import UIKit
import UniformTypeIdentifiers

/// Bridge between a UIDocumentPickerViewController and its Pigeon callback.
class PickerCompletionBridge: NSObject, UIDocumentPickerDelegate {
  let completion: (Result<[String], Error>) -> Void
  /// The plugin instance that owns this object, to ensure that it lives as long as the picker it
  /// serves as a delegate for. Instances are responsible for removing themselves from their owner
  /// on completion.
  let owner: FileSelectorPlugin

  init(completion: @escaping (Result<[String], Error>) -> Void, owner: FileSelectorPlugin) {
    self.completion = completion
    self.owner = owner
  }

  func documentPicker(
    _ controller: UIDocumentPickerViewController,
    didPickDocumentsAt urls: [URL]
  ) {
    sendResult(urls.map({ $0.path }))
  }

  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    sendResult([])
  }

  private func sendResult(_ result: [String]) {
    completion(.success(result))
    owner.pendingCompletions.remove(self)
  }
}

/// Bridge for the repository-owned directory picker method channel.
final class DirectoryPickerCompletionBridge: NSObject, UIDocumentPickerDelegate {
  let completion: FlutterResult
  let owner: FileSelectorPlugin
  let multiple: Bool

  init(completion: @escaping FlutterResult, owner: FileSelectorPlugin, multiple: Bool) {
    self.completion = completion
    self.owner = owner
    self.multiple = multiple
  }

  func documentPicker(
    _ controller: UIDocumentPickerViewController,
    didPickDocumentsAt urls: [URL]
  ) {
    do {
      let selected = try urls.map { try IOSFolderAccessStore.shared.rememberSelection($0).path }
      completion(multiple ? selected : Array(selected.prefix(1)))
    } catch {
      completion(FlutterError(
        code: "FOLDER_ACCESS_ERROR",
        message: error.localizedDescription,
        details: nil
      ))
    }
    owner.pendingDirectoryCompletions.remove(self)
  }

  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    completion([String]())
    owner.pendingDirectoryCompletions.remove(self)
  }
}

public class FileSelectorPlugin: NSObject, FlutterPlugin, FileSelectorApi {
  /// Owning references to pending completion callbacks.
  ///
  /// This is necessary since the objects need to live until a UIDocumentPickerDelegate method is
  /// called on the delegate, but the delegate is weak. Objects in this set are responsible for
  /// removing themselves from it.
  var pendingCompletions: Set<PickerCompletionBridge> = []
  var pendingDirectoryCompletions: Set<DirectoryPickerCompletionBridge> = []
  /// Overridden document picker, for testing.
  var documentPickerViewControllerOverride: UIDocumentPickerViewController?
  /// The view controller provider, for showing the document picker.
  let viewPresenterProvider: ViewPresenterProvider
  private var directoryChannel: FlutterMethodChannel?

  public static func register(with registrar: FlutterPluginRegistrar) {
    let instance = FileSelectorPlugin(
      viewPresenterProvider: DefaultViewPresenterProvider(registrar: registrar))
    FileSelectorApiSetup.setUp(binaryMessenger: registrar.messenger(), api: instance)
    instance.installDirectoryChannel(binaryMessenger: registrar.messenger())
  }

  init(viewPresenterProvider: ViewPresenterProvider) {
    self.viewPresenterProvider = viewPresenterProvider
  }

  func openFile(config: FileSelectorConfig, completion: @escaping (Result<[String], Error>) -> Void)
  {
    let completionBridge = PickerCompletionBridge(completion: completion, owner: self)
    let documentPicker =
      documentPickerViewControllerOverride
      ?? UIDocumentPickerViewController(
        documentTypes: config.utis,
        in: .import)
    documentPicker.allowsMultipleSelection = config.allowMultiSelection
    documentPicker.delegate = completionBridge

    if let presenter = viewPresenterProvider.viewPresenter {
      pendingCompletions.insert(completionBridge)
      presenter.present(documentPicker, animated: true, completion: nil)
    } else {
      completion(
        .failure(PigeonError(code: "error", message: "No view controller available.", details: nil))
      )
    }
  }

  private func installDirectoryChannel(binaryMessenger: FlutterBinaryMessenger) {
    let channel = FlutterMethodChannel(
      name: "dev.flutter.packages.file_selector_ios/directory",
      binaryMessenger: binaryMessenger
    )
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handleDirectoryCall(call, result: result)
    }
    directoryChannel = channel
  }

  private func handleDirectoryCall(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    guard call.method == "pickDirectory" else {
      result(FlutterMethodNotImplemented)
      return
    }
    guard pendingDirectoryCompletions.isEmpty else {
      result(FlutterError(
        code: "PICK_IN_PROGRESS",
        message: "A directory picker is already open.",
        details: nil
      ))
      return
    }
    let arguments = call.arguments as? [String: Any]
    let multiple = arguments?["multiple"] as? Bool ?? false
    let bridge = DirectoryPickerCompletionBridge(
      completion: result,
      owner: self,
      multiple: multiple
    )
    let picker = UIDocumentPickerViewController(
      forOpeningContentTypes: [UTType.folder],
      asCopy: false
    )
    if let initialPath = arguments?["initialDirectory"] as? String,
       (initialPath as NSString).isAbsolutePath {
      picker.directoryURL = URL(fileURLWithPath: initialPath)
    }
    picker.allowsMultipleSelection = multiple
    picker.delegate = bridge
    guard let presenter = viewPresenterProvider.viewPresenter else {
      result(FlutterError(
        code: "PICK_UNAVAILABLE",
        message: "No view controller available.",
        details: nil
      ))
      return
    }
    pendingDirectoryCompletions.insert(bridge)
    presenter.present(picker, animated: true, completion: nil)
  }

}

/// Keeps iOS security-scoped directory access active while the app process runs.
final class IOSFolderAccessStore {
  static let shared = IOSFolderAccessStore()
  private var activeURLs: [String: URL] = [:]

  func rememberSelection(_ inputURL: URL) throws -> URL {
    let url = inputURL.standardizedFileURL
    let path = url.path
    if activeURLs[path] != nil { return url }
    guard url.startAccessingSecurityScopedResource() else {
      throw NSError(
        domain: "file_selector_ios",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "iOS did not grant access to the selected folder: \(path)"]
      )
    }
    do {
      activeURLs[path] = url
      return url
    } catch {
      url.stopAccessingSecurityScopedResource()
      throw error
    }
  }

}
