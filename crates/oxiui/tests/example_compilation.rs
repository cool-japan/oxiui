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
