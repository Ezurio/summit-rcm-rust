use std::fs;
use std::path::{Path, PathBuf};

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("failed to read directory");
    for entry in entries {
        let entry = entry.expect("failed to read directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

// Files that wrap bindgen-generated code and legitimately suppress dead_code
// for constants that are only partially used at any given time.
const DEAD_CODE_EXCEPTIONS: &[&str] = &["plugins/network/nl80211/protocol.rs"];

fn is_exception(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    DEAD_CODE_EXCEPTIONS
        .iter()
        .any(|suffix| path_str.replace('\\', "/").ends_with(suffix))
}

#[test]
fn source_tree_has_no_dead_code_allows() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut files = Vec::new();
    collect_rs_files(&src_dir, &mut files);

    let mut violations = Vec::new();
    for file in files {
        if is_exception(&file) {
            continue;
        }
        let content = fs::read_to_string(&file).expect("failed to read source file");
        if content.contains("allow(dead_code)") {
            violations.push(file);
        }
    }

    assert!(
        violations.is_empty(),
        "Found forbidden allow(dead_code) attributes in: {:?}",
        violations
    );
}
