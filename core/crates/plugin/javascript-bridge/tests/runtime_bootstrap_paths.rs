use operit_js_bridge::javascript::JsLibraries::{
    buildRuntimeBootstrapModules, buildRuntimeBootstrapScript,
};

const CLEAN_ON_EXIT_ASSIGNMENT: &str =
    "var OPERIT_CLEAN_ON_EXIT_DIR = \"/app/data/temp/clean_on_exit\";";

#[test]
fn modular_bootstrap_exposes_clean_on_exit_as_a_vfs_path() {
    // No storage host is registered: the public path must neither require one
    // nor capture a physical identity root that could become stale later.
    let modules = buildRuntimeBootstrapModules();
    let paths_module = modules
        .iter()
        .find(|module| module.fileName == "quickjs/init/operit-paths.js")
        .expect("paths bootstrap module must be present");
    assert!(paths_module.source.contains(CLEAN_ON_EXIT_ASSIGNMENT));
    assert_eq!(paths_module.globals, vec!["OPERIT_CLEAN_ON_EXIT_DIR"]);
}

#[test]
fn monolithic_bootstrap_exposes_the_same_clean_on_exit_vfs_path() {
    assert!(buildRuntimeBootstrapScript().contains(CLEAN_ON_EXIT_ASSIGNMENT));
}
