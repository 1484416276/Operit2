import WebKit
#if os(iOS)
import Flutter
#else
import FlutterMacOS
#endif

/// Delivers VFS resources on the WebKit scheme-task lifecycle, without sockets.
final class WebViewLocalResources: NSObject, WKURLSchemeHandler {
  private let channel: FlutterMethodChannel
  private unowned let registrar: ProxyAPIRegistrar
  private var enabled = Set<Int64>()
  private var tasks = Set<ObjectIdentifier>()

  init(messenger: FlutterBinaryMessenger, registrar: ProxyAPIRegistrar) {
    self.channel = FlutterMethodChannel(name: "operit/webview_resources", binaryMessenger: messenger)
    self.registrar = registrar
    super.init()
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else { result(FlutterError(code: "detached", message: nil, details: nil)); return }
      guard call.method == "configure", let args = call.arguments as? [String: Any],
        let id = args["identifier"] as? NSNumber, let enable = args["enabled"] as? Bool
      else { result(FlutterMethodNotImplemented); return }
      if enable { self.enabled.insert(id.int64Value) } else { self.enabled.remove(id.int64Value) }
      result(nil)
    }
  }

  func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
    guard let url = task.request.url,
      url.host?.hasSuffix(".operit.invalid") == true,
      let id = registrar.instanceManager.identifierWithStrongReference(forInstance: webView),
      enabled.contains(id)
    else { task.didFailWithError(URLError(.noPermissionsToReadFile)); return }
    let token = ObjectIdentifier(task)
    tasks.insert(token)
    var completed = false
    let finish: (Any?) -> Void = { [weak self] value in
      guard let self, !completed else { return }
      completed = true
      guard self.tasks.remove(token) != nil else { return }
      guard let response = value as? [String: Any],
        let body = response["body"] as? FlutterStandardTypedData
      else { task.didFailWithError(URLError(.cannotLoadFromNetwork)); return }
      var headers = response["headers"] as? [String: String] ?? [:]
      if !headers.keys.contains(where: { $0.lowercased() == "content-type" }) {
        headers["Content-Type"] = "\(response["mimeType"] as? String ?? "application/octet-stream"); charset=\(response["encoding"] as? String ?? "utf-8")"
      }
      guard let nativeResponse = HTTPURLResponse(url: url,
        statusCode: response["statusCode"] as? Int ?? 200,
        httpVersion: "HTTP/1.1", headerFields: headers)
      else { task.didFailWithError(URLError(.badServerResponse)); return }
      task.didReceive(nativeResponse)
      task.didReceive(body.data)
      task.didFinish()
    }
    channel.invokeMethod("request", arguments: [
      "identifier": id, "url": url.absoluteString,
      "method": task.request.httpMethod ?? "GET",
      "headers": task.request.allHTTPHeaderFields ?? [:],
      "isMainFrame": task.request.mainDocumentURL == url,
    ], result: finish)
    DispatchQueue.main.asyncAfter(deadline: .now() + 35) { finish(nil) }
  }

  func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {
    tasks.remove(ObjectIdentifier(task))
  }
}
