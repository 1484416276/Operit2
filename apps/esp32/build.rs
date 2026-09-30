/// Exports the ESP-IDF SDK environment when building the firmware target.
fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("CARGO_MANIFEST_DIR must be set");
    let token_file = manifest_dir.join(".edge-token");
    println!("cargo:rerun-if-env-changed=OPERIT_WIFI_SSID");
    println!("cargo:rerun-if-env-changed=OPERIT_WIFI_PASSWORD");
    println!("cargo:rerun-if-env-changed=OPERIT_EDGE_TOKEN");
    println!("cargo:rerun-if-env-changed=OPERIT_TIMEZONE");
    println!("cargo:rerun-if-changed={}", token_file.display());
    // The Edge token belongs to the Core/CLI deployment, so use the checked
    // out deployment secret as the build input when no environment override
    // was supplied. Do not print the token into build logs.
    if std::env::var_os("OPERIT_EDGE_TOKEN").is_none() {
        if let Ok(token) = std::fs::read_to_string(&token_file) {
            let token = token.trim();
            if !token.is_empty() {
                println!("cargo:rustc-env=OPERIT_EDGE_TOKEN={token}");
            }
        }
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }
}
