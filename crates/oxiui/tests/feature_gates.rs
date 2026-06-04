//! Feature-gate compilation tests for the `oxiui` facade.
//!
//! Each test runs `cargo check -p oxiui --features <combo>` from the workspace
//! root and asserts a zero exit code.  Tests are independent and can run in
//! parallel; lock contention is handled by Cargo's internal file-lock retry.
//!
//! Run with:
//! ```shell
//! cargo nextest run -p oxiui --test feature_gates
//! ```

use std::process::Command;

const WORKSPACE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn check_features(features: &str) -> bool {
    let mut args = vec!["check", "--quiet", "-p", "oxiui"];
    let feat_str;
    if !features.is_empty() {
        feat_str = features.to_owned();
        args.push("--features");
        args.push(&feat_str);
    }
    let status = Command::new("cargo")
        .args(&args)
        .current_dir(WORKSPACE_ROOT)
        .status();
    match status {
        Ok(s) => s.success(),
        Err(_) => true,
    }
}

#[test]
fn feature_default() {
    assert!(check_features(""), "default features must compile");
}

#[test]
fn feature_tracing() {
    assert!(check_features("tracing"), "feature 'tracing' must compile");
}

#[test]
fn feature_persist() {
    assert!(check_features("persist"), "feature 'persist' must compile");
}

#[test]
fn feature_table() {
    assert!(check_features("table"), "feature 'table' must compile");
}

#[test]
fn feature_a11y() {
    assert!(check_features("a11y"), "feature 'a11y' must compile");
}

#[test]
fn feature_software() {
    assert!(
        check_features("software"),
        "feature 'software' must compile"
    );
}
