//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Emit `extern crate <plugin> as _;` for each enabled plugin crate so the
//! linker keeps its `inventory::submit!` registrations. The manifest's
//! `summit-rcm-*` dependencies (minus core) are the single source of truth.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    let links: String = fs::read_to_string("Cargo.toml")
        .unwrap()
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("summit-rcm-")
                .and_then(|r| r.split_once(' '))
                .map(|(n, _)| n)
        })
        .filter(|n| {
            *n != "core"
                && env::var(format!(
                    "CARGO_FEATURE_{}",
                    n.to_uppercase().replace('-', "_")
                ))
                .is_ok()
        })
        .map(|n| format!("extern crate summit_rcm_{} as _;\n", n.replace('-', "_")))
        .collect();
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("plugin_links.rs"),
        links,
    )
    .unwrap();
}
