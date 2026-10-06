// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
// Host tool: generates src/plugins/network/nl80211/generated.rs via bindgen.
//
// Run with:
//   cargo run --bin generate_nl80211_bindings
//
// Optional positional argument overrides the output path.
// Set NL80211_INCLUDE_DIR / STAGING_DIR / BR2_SYSROOT / SDKTARGETSYSROOT /
// OECORE_TARGET_SYSROOT to point at a cross sysroot when needed.

use std::path::PathBuf;

fn main() {
    let output_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/plugins/network/nl80211/generated.rs")
        });

    let (header_path, include_dirs) = resolve_nl80211_header_and_includes();

    const ENUMS: &[&str] = &[
        "nl80211_commands",
        "nl80211_attrs",
        "nl80211_iftype",
        "nl80211_sta_info",
        "nl80211_sta_bss_param",
        "nl80211_rate_info",
        "nl80211_band_attr",
        "nl80211_frequency_attr",
        "nl80211_reg_rule_attr",
    ];

    let mut builder = bindgen::Builder::default()
        .header(header_path.to_string_lossy().into_owned())
        .layout_tests(false)
        .generate_comments(false)
        .formatter(bindgen::Formatter::Prettyplease);

    for name in ENUMS {
        builder = builder.allowlist_type(name).constified_enum_module(name);
    }

    for include_dir in include_dirs {
        builder = builder.clang_arg(format!("-I{}", include_dir.display()));
    }

    let bindings = builder.generate().unwrap_or_else(|e| {
        panic!(
            "unable to generate bindings from {}: {e}",
            header_path.display()
        )
    });

    bindings
        .write_to_file(&output_path)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", output_path.display()));

    eprintln!("wrote {}", output_path.display());
}

fn resolve_nl80211_header_and_includes() -> (PathBuf, Vec<PathBuf>) {
    let include_dirs = include_dirs_from_env_and_defaults();
    for include_dir in &include_dirs {
        let candidate = include_dir.join("linux/nl80211.h");
        if candidate.is_file() {
            return (candidate, include_dirs);
        }
    }
    panic!(
        "unable to locate linux/nl80211.h; set NL80211_INCLUDE_DIR, STAGING_DIR, \
         BR2_SYSROOT, SDKTARGETSYSROOT, or OECORE_TARGET_SYSROOT to the sysroot prefix"
    );
}

fn include_dirs_from_env_and_defaults() -> Vec<PathBuf> {
    let prefix = selected_prefix();
    [
        prefix.join("usr/include/linux-backports/include"),
        prefix.join("usr/include/uapi"),
        prefix.join("usr/include"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

fn selected_prefix() -> PathBuf {
    for env_name in [
        "NL80211_INCLUDE_DIR",
        "STAGING_DIR",
        "BR2_SYSROOT",
        "SDKTARGETSYSROOT",
        "OECORE_TARGET_SYSROOT",
    ] {
        if let Ok(value) = std::env::var(env_name)
            && !value.trim().is_empty()
        {
            return PathBuf::from(value);
        }
    }
    PathBuf::from("/")
}
