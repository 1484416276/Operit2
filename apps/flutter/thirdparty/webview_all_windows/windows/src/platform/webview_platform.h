#pragma once

#include <winrt/base.h>

#include <memory>
#include <optional>
#include <string>

#include "platform/winrt_runtime.h"

namespace webview_all_windows {

class WebviewPlatform {
public:
  WebviewPlatform();
  bool IsSupported() { return valid_; }
  std::optional<std::wstring> GetDefaultDataDirectory();

  WinrtRuntime *runtime() const { return runtime_.get(); }

private:
  std::unique_ptr<WinrtRuntime> runtime_;
  winrt::com_ptr<ABI::Windows::System::IDispatcherQueueController>
      dispatcher_queue_controller_;
  bool valid_ = false;
};

} // namespace webview_all_windows
