use crate::output::CoreCommandOutput;
use operit_host_api::HostManager::HostManager;
use operit_runtime::core::application::OperitApplication::OperitApplication;

pub fn run_host_command(
    context: HostManager,
    args: &[String],
    output: &mut CoreCommandOutput,
) -> Result<(), String> {
    if args.is_empty() {
        print_host_usage(output);
        return Ok(());
    }

    match args[0].as_str() {
        "show" => {
            let targetOs = std::env::consts::OS;
            let targetArch = std::env::consts::ARCH;
            let coreVersion = OperitApplication::newWithContext(context).coreVersion();
            output.push_stdout_line(format!("Target OS: {targetOs}"));
            output.push_stdout_line(format!("Target arch: {targetArch}"));
            output.push_stdout_line(format!("Core version: {coreVersion}"));
            output.setJsonStdout(serde_json::json!({
                "targetOs": targetOs,
                "targetArch": targetArch,
                "coreVersion": coreVersion,
            }));
            Ok(())
        }
        "capabilities" => Err("host capabilities are not exposed by core command".to_string()),
        "paths" => Err("host paths are not exposed by core command".to_string()),
        _ => {
            print_host_usage(output);
            Ok(())
        }
    }
}

/// Prints host command usage.
///
/// `capabilities` and `paths` are intentionally absent: both subcommands are
/// hard errors on the core side because host capabilities/paths are not
/// exposed through core commands (see `run_host_command` and
/// `core/CRATE_BOUNDARIES.md`).
fn print_host_usage(output: &mut CoreCommandOutput) {
    let lines = vec!["operit2 host show"];
    for line in &lines {
        output.push_stdout_line(line);
    }
    output.setJsonStdout(serde_json::json!({"usage": lines}));
}
