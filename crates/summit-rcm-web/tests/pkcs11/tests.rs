use super::{convert_pkcs11_uri_to_pem, pkcs11_uri_to_pem_bytes};
use base64::Engine as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn converts_private_key_pkcs11_uri_to_pem() {
    let pem = pkcs11_uri_to_pem_bytes("pkcs11:token=MyToken;object=MyObject;type=private")
        .expect("valid pkcs11 uri should encode");
    let pem = String::from_utf8(pem).expect("PEM should be valid UTF-8");

    assert!(pem.starts_with("-----BEGIN PKCS#11 PROVIDER URI-----\n"));
    assert!(pem.ends_with("-----END PKCS#11 PROVIDER URI-----\n"));

    let body = pem
        .lines()
        .skip(1)
        .take_while(|line| *line != "-----END PKCS#11 PROVIDER URI-----")
        .collect::<String>();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body)
        .expect("PEM body should be base64");

    assert_eq!(der[0], 0x30, "wrapper should be an ASN.1 sequence");
    assert!(
        der.windows(b"PKCS#11 Provider URI v1.0".len())
            .any(|window| window == b"PKCS#11 Provider URI v1.0"),
        "visible-string description should be embedded"
    );
    assert!(
        der.windows(b"pkcs11:token=MyToken;object=MyObject;type=private".len())
            .any(|window| window == b"pkcs11:token=MyToken;object=MyObject;type=private"),
        "UTF8String URI should be embedded"
    );
}

#[test]
fn converts_certificate_pkcs11_uri_to_pem() {
    let pem = pkcs11_uri_to_pem_bytes("pkcs11:token=MyToken;object=MyCert;type=cert")
        .expect("certificate pkcs11 uri should encode");
    let pem = String::from_utf8(pem).expect("PEM should be valid UTF-8");

    assert!(pem.starts_with("-----BEGIN PKCS#11 PROVIDER URI-----\n"));
    assert!(pem.ends_with("-----END PKCS#11 PROVIDER URI-----\n"));
}

#[test]
fn accepts_public_key_pkcs11_uri() {
    pkcs11_uri_to_pem_bytes("pkcs11:object=pubkey;type=public")
        .expect("public key URI should also be wrapped");
}

#[test]
fn rejects_non_pkcs11_uri() {
    let error = pkcs11_uri_to_pem_bytes("file:/tmp/key.pem")
        .expect_err("non-pkcs11 URI should be rejected")
        .to_string();
    assert!(error.contains("not a valid PKCS#11 URI"));
}

#[test]
fn rejects_uri_without_object_or_id() {
    let error = pkcs11_uri_to_pem_bytes("pkcs11:token=MyToken;type=private")
        .expect_err("URI missing object/id should be rejected")
        .to_string();
    assert!(error.contains("does not specify an object by label or id"));
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "summit-rcm-{prefix}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("temp dir should be creatable");
    path
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[tokio::test]
async fn rust_converter_matches_uri2pem_py_output() {
    let temp_dir = unique_temp_dir("uri2pem-compare");
    let rust_output = temp_dir.join("rust.pem");
    let python_output = temp_dir.join("python.pem");
    let fixtures = fixtures_dir();
    let script = fixtures.join("uri2pem.py");
    let uri = "pkcs11:token=MyToken;object=MyObject;type=private";

    convert_pkcs11_uri_to_pem(uri, &rust_output)
        .await
        .expect("Rust converter should succeed");

    let status = Command::new("python3")
        .env("PYTHONPATH", &fixtures)
        .arg(&script)
        .arg("--out")
        .arg(&python_output)
        .arg(uri)
        .status()
        .expect("python3 should run uri2pem.py fixture");
    assert!(status.success(), "uri2pem.py fixture should succeed");

    let rust_bytes = std::fs::read(&rust_output).expect("Rust output should be readable");
    let python_bytes = std::fs::read(&python_output).expect("Python output should be readable");

    assert_eq!(
        rust_bytes, python_bytes,
        "Rust and Python converters must produce identical PEM files"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
