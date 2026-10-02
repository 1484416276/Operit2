use std::sync::Arc;

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use operit_host_api::setHostConsoleLogSink;

#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn NSLog(format: CFStringRef, ...);
}

/// Uses Apple's native console instead of Rust stdout, which Flutter's iOS
/// debugger does not reliably forward. Runtime file logging remains unchanged.
pub fn installAppleLogSink() {
    setHostConsoleLogSink(Arc::new(|_priority, _tag, message| {
        logToAppleConsole(message);
    }));
}

fn logToAppleConsole(message: &str) {
    let format = CFString::new("%@");
    let message = CFString::new(message.trim_end_matches(['\r', '\n']));
    // CFString is toll-free bridged with NSString. The format is constant:
    // percent signs, Unicode, and embedded NULs in log text remain plain data.
    unsafe { NSLog(format.as_concrete_TypeRef(), message.as_concrete_TypeRef()) };
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_console_accepts_unicode_percent_and_nul() {
        super::logToAppleConsole("I/ChatCreate: 新聊天 100% %@ %s \0\n");
    }
}
