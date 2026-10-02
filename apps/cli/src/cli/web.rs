use super::{cli_json_mode, emit_cli_json, open_local_url};

const WEB_APP_URL: &str = "https://web.operit.app/";

/// 网页入口只打开在线应用；监听、发现、凭证和配对全部由节点服务管理。
pub(super) fn run_web_command(args: &[String]) -> Result<(), String> {
    if !matches!(args, [command] if command == "open") {
        return Err("usage: operit2 cli web open".into());
    }
    if cli_json_mode() {
        emit_cli_json(serde_json::json!({ "url": WEB_APP_URL }));
    } else {
        println!("{WEB_APP_URL}");
        open_local_url(WEB_APP_URL);
    }
    Ok(())
}
