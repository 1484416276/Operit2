#pragma once

#include <WebView2.h>
#include <WebView2EnvironmentOptions.h>
#include <wil/com.h>

#include <functional>
#include <string>
#include <vector>

#include "platform/webview_platform.h"
#include "webview/webview.h"

namespace webview_all_windows {

struct WebviewCreationError {
  HRESULT hr;
  std::string message;

  explicit WebviewCreationError(HRESULT hr, std::string message)
      : hr(hr), message(message) {}

  static std::unique_ptr<WebviewCreationError>
  create(HRESULT hr, const std::string message) {
    return std::make_unique<WebviewCreationError>(hr, message);
  }
};

class WebviewHost {
public:
  typedef std::function<void(std::unique_ptr<Webview>,
                             std::unique_ptr<WebviewCreationError>)>
      WebviewCreationCallback;
  typedef std::function<void(wil::com_ptr<ICoreWebView2CompositionController>,
                             std::unique_ptr<WebviewCreationError>)>
      CompositionControllerCreationCallback;
  typedef std::function<void(wil::com_ptr<ICoreWebView2PointerInfo>,
                             std::unique_ptr<WebviewCreationError>)>
      PointerInfoCreationCallback;

  /// Creates the environment asynchronously on the browser STA thread.
  static void Create(
      std::optional<std::wstring> user_data_directory,
      std::optional<std::wstring> browser_exe_path,
      std::optional<std::string> arguments,
      std::function<void(std::shared_ptr<WebviewHost>, HRESULT)> callback);

  void CreateWebview(HWND hwnd, bool offscreen_only, bool owns_window,
                     WebviewCreationCallback callback);

  void CreateWebViewPointerInfo(PointerInfoCreationCallback cb);

  wil::com_ptr<ICoreWebView2WebResourceRequest>
  CreateWebResourceRequest(const std::string &url, const std::string &method,
                           const std::string &headers,
                           const std::vector<uint8_t> *body);

private:
  wil::com_ptr<ICoreWebView2Environment3> webview_env_;

  explicit WebviewHost(wil::com_ptr<ICoreWebView2Environment3> webview_env);
  void
  CreateWebViewCompositionController(HWND hwnd,
                                     CompositionControllerCreationCallback cb);
};

} // namespace webview_all_windows
