use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let cargo_toml = fs::read_to_string("../../Cargo.toml").unwrap();
    let public_version = workspace_public_version(&cargo_toml)
        .expect("Cargo.toml must define [workspace.metadata.summit-rcm].public-version");
    println!("cargo:rustc-env=SUMMIT_RCM_PUBLIC_VERSION={public_version}");
}

fn workspace_public_version(cargo_toml: &str) -> Option<String> {
    let mut in_section = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == "[workspace.metadata.summit-rcm]";
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(value) = trimmed.strip_prefix("public-version") else {
            continue;
        };
        let value = value.trim_start();
        let value = value.strip_prefix('=')?.trim();
        return value.strip_prefix('"')?.strip_suffix('"').map(str::to_string);
    }
    None
}