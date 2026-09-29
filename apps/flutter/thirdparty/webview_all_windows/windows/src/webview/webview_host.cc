#include "webview/webview_host.h"

#include <wrl.h>

#include <cstring>
#include <future>

#include "util/string_converter.h"

using namespace Microsoft::WRL;

namespace webview_all_windows {

// Creates WebView2 without blocking the STA message loop needed by its callback.
void WebviewHost::Create(
    std::optional<std::wstring> user_data_directory,
    std::optional<std::wstring> browser_exe_path,
    std::optional<std::string> arguments,
    std::function<void(std::shared_ptr<WebviewHost>, HRESULT)> callback) {
  auto options = Microsoft::WRL::Make<CoreWebView2EnvironmentOptions>();
  if (arguments) {
    const auto value = util::Utf16FromUtf8(*arguments);
    options->put_AdditionalBrowserArguments(value.c_str());
  }
  const HRESULT started = CreateCoreWebView2EnvironmentWithOptions(
      browser_exe_path ? browser_exe_path->c_str() : nullptr,
      user_data_directory ? user_data_directory->c_str() : nullptr,
      options.Get(),
      Callback<ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler>(
          [callback](HRESULT error, ICoreWebView2Environment* environment) -> HRESULT {
            if (FAILED(error) || !environment) {
              callback(nullptr, FAILED(error) ? error : E_POINTER);
              return S_OK;
            }
            wil::com_ptr<ICoreWebView2Environment3> environment3;
            const HRESULT query = environment->QueryInterface(IID_PPV_ARGS(&environment3));
            if (FAILED(query)) {
              callback(nullptr, query);
              return S_OK;
            }
            callback(std::shared_ptr<WebviewHost>(new WebviewHost(std::move(environment3))), S_OK);
            return S_OK;
          }).Get());
  if (FAILED(started)) callback(nullptr, started);
}

// Retains the environment used by all controllers owned by this plugin.
WebviewHost::WebviewHost(wil::com_ptr<ICoreWebView2Environment3> environment)
    : webview_env_(std::move(environment)) {}

void WebviewHost::CreateWebview(HWND hwnd, bool offscreen_only,
                                bool owns_window,
                                WebviewCreationCallback callback) {
  CreateWebViewCompositionController(
      hwnd, [=, self = this](
                wil::com_ptr<ICoreWebView2CompositionController> controller,
                std::unique_ptr<WebviewCreationError> error) {
        if (controller) {
          std::unique_ptr<Webview> webview(new Webview(
              std::move(controller), self, hwnd, owns_window, offscreen_only));
          callback(std::move(webview), nullptr);
        } else {
          callback(nullptr, std::move(error));
        }
      });
}

void WebviewHost::CreateWebViewPointerInfo(
    PointerInfoCreationCallback callback) {

  ICoreWebView2PointerInfo *pointer;
  auto hr = webview_env_->CreateCoreWebView2PointerInfo(&pointer);

  if (FAILED(hr)) {
    callback(nullptr, WebviewCreationError::create(
                          hr, "CreateWebViewPointerInfo failed."));
  } else if (SUCCEEDED(hr)) {
    callback(std::move(wil::com_ptr<ICoreWebView2PointerInfo>(pointer)),
             nullptr);
  }
}

wil::com_ptr<ICoreWebView2WebResourceRequest>
WebviewHost::CreateWebResourceRequest(const std::string &url,
                                      const std::string &method,
                                      const std::string &headers,
                                      const std::vector<uint8_t> *body) {
  wil::com_ptr<IStream> body_stream;
  if (body != nullptr && !body->empty()) {
    HGLOBAL global = GlobalAlloc(GMEM_MOVEABLE, body->size());
    if (global == nullptr) {
      return nullptr;
    }

    void *data = GlobalLock(global);
    if (data == nullptr) {
      GlobalFree(global);
      return nullptr;
    }
    std::memcpy(data, body->data(), body->size());
    GlobalUnlock(global);

    IStream *stream = nullptr;
    if (FAILED(CreateStreamOnHGlobal(global, TRUE, &stream))) {
      GlobalFree(global);
      return nullptr;
    }
    body_stream.attach(stream);
  }

  wil::com_ptr<ICoreWebView2WebResourceRequest> request;
  if (FAILED(webview_env_->CreateWebResourceRequest(
          util::Utf16FromUtf8(url).c_str(), util::Utf16FromUtf8(method).c_str(),
          body_stream.get(), util::Utf16FromUtf8(headers).c_str(),
          request.put()))) {
    return nullptr;
  }
  return request;
}

void WebviewHost::CreateWebViewCompositionController(
    HWND hwnd, CompositionControllerCreationCallback callback) {
  auto hr = webview_env_->CreateCoreWebView2CompositionController(
      hwnd,
      Callback<
          ICoreWebView2CreateCoreWebView2CompositionControllerCompletedHandler>(
          [callback](HRESULT hr,
                     ICoreWebView2CompositionController *compositionController)
              -> HRESULT {
            if (SUCCEEDED(hr)) {
              callback(
                  std::move(wil::com_ptr<ICoreWebView2CompositionController>(
                      compositionController)),
                  nullptr);
            } else {
              callback(nullptr,
                       WebviewCreationError::create(
                           hr, "CreateCoreWebView2CompositionController "
                               "completion handler failed."));
            }

            return S_OK;
          })
          .Get());

  if (FAILED(hr)) {
    callback(nullptr,
             WebviewCreationError::create(
                 hr, "CreateCoreWebView2CompositionController failed."));
  }
}

} // namespace webview_all_windows
