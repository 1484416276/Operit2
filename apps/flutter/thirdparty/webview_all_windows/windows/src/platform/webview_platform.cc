#include "platform/webview_platform.h"

#include <DispatcherQueue.h>
#include <shlobj.h>

#include <filesystem>

#include "util/logging.h"

namespace webview_all_windows {

WebviewPlatform::WebviewPlatform()
    : runtime_(std::make_unique<WinrtRuntime>(RO_INIT_SINGLETHREADED)) {
  if (runtime_->available()) {
    DispatcherQueueOptions options{sizeof(DispatcherQueueOptions),
                                   DQTYPE_THREAD_CURRENT, DQTAT_COM_STA};

    if (FAILED(runtime_->CreateDispatcherQueueController(
            options, dispatcher_queue_controller_.put()))) {
      util::LogWarning("Creating DispatcherQueueController failed.");
      return;
    }

    valid_ = true;
  }
}

std::optional<std::wstring> WebviewPlatform::GetDefaultDataDirectory() {
  PWSTR path_tmp;
  if (!SUCCEEDED(
          SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, nullptr, &path_tmp))) {
    return std::nullopt;
  }
  auto path = std::filesystem::path(path_tmp);
  CoTaskMemFree(path_tmp);

  wchar_t filename[MAX_PATH];
  GetModuleFileName(nullptr, filename, MAX_PATH);
  path /= "webview_all_windows";
  path /= std::filesystem::path(filename).stem();

  return path.wstring();
}

} // namespace webview_all_windows
