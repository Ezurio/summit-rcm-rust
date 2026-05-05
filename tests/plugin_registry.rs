use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn registered_builtin_plugins() -> BTreeSet<String> {
    fs::read_to_string(concat!(env!("OUT_DIR"), "/builtin_plugins.rs"))
        .unwrap()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub mod "))
        .filter_map(|line| line.strip_suffix(';'))
        .map(|module_name| module_name.replace('_', "-"))
        .collect()
}

fn source_tree_builtin_plugins() -> BTreeSet<String> {
    fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/plugins"))
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| {
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                return path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .map(|name| name.replace('_', "-"));
            }

            if path.is_dir() && path.join("mod.rs").is_file() {
                return path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.replace('_', "-"));
            }

            None
        })
        .filter(|module_name| module_name != "mod")
        .filter(|module_name| module_name != "systemd-state")
        .collect()
}

#[test]
fn builtin_plugin_registry_matches_source_tree() {
    assert_eq!(registered_builtin_plugins(), source_tree_builtin_plugins());
}
