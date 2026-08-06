//! Example compilation gate tests.
//!
//! Each test verifies that the corresponding example in `examples/` compiles
//! without error.  Uses `cargo build -p oxiui --example <name>` so the test
//! targets only the `oxiui` package within the workspace.
//!
//! Run with:
//! ```shell
//! cargo nextest run -p oxiui --test example_compilation
//! ```

use std::process::Command;

/// Workspace root: one level above the crate (../Cargo.toml).
const WORKSPACE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn build_example(name: &str, features: &str) -> bool {
    let mut args = vec!["build", "--quiet", "-p", "oxiui", "--example", name];
    // Only add --features when non-empty.
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
        // cargo unavailable — skip gracefully.
        Err(_) => true,
    }
}

#[test]
fn example_hello_compiles() {
    assert!(build_example("hello", ""), "example 'hello' must compile");
}

#[test]
fn example_hello_iced_compiles() {
    assert!(
        build_example("hello_iced", "iced"),
        "example 'hello_iced' must compile"
    );
}

#[test]
fn example_hello_table_compiles() {
    assert!(
        build_example("hello_table", "table"),
        "example 'hello_table' must compile"
    );
}

#[test]
fn example_hello_headless_compiles() {
    assert!(
        build_example("hello_headless", "software"),
        "example 'hello_headless' must compile"
    );
}

/// Runs the built `hello_headless` example and asserts it exits successfully.
///
/// `hello_headless` is the M5 "headless smoke layer" gate: it renders a frame
/// with no window/GPU/display and calls `std::process::exit(1)` if the buffer
/// doesn't contain visible (non-background) content. `Dockerfile.ffi-audit`
/// runs this same command as its final smoke-test layer; this test gives the
/// same assertion durable, non-Docker-dependent coverage in the normal test
/// suite (Docker may not be available in every CI/dev environment).
#[test]
fn example_hello_headless_runs_and_exits_ok() {
    let build_status = Command::new("cargo")
        .args([
            "build",
            "--quiet",
            "-p",
            "oxiui",
            "--example",
            "hello_headless",
            "--features",
            "software",
        ])
        .current_dir(WORKSPACE_ROOT)
        .status();
    let Ok(build_status) = build_status else {
        // cargo unavailable — skip gracefully.
        return;
    };
    assert!(build_status.success(), "hello_headless must build first");

    let run_status = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "-p",
            "oxiui",
            "--example",
            "hello_headless",
            "--features",
            "software",
        ])
        .current_dir(WORKSPACE_ROOT)
        .status();
    // cargo unavailable — skip gracefully; otherwise assert a clean exit.
    if let Ok(s) = run_status {
        assert!(
            s.success(),
            "hello_headless must exit 0 (non-zero means it rendered no visible content)"
        );
    }
}
