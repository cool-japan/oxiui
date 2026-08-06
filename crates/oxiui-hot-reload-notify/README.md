# oxiui-hot-reload-notify — WGSL shader hot-reload file watcher (quarantine crate)

[![Crates.io](https://img.shields.io/crates/v/oxiui-hot-reload-notify.svg)](https://crates.io/crates/oxiui-hot-reload-notify)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-hot-reload-notify` is the WGSL shader hot-reload file watcher for the COOLJAPAN OxiUI ecosystem. `ShaderWatcher` wraps a `notify::RecommendedWatcher` and tracks a set of WGSL source paths; whenever a watched file is modified on disk, its path is pushed into a shared changed-set that the caller drains once per frame to know which pipelines to recompile.

## Pure-Rust quarantine notice

This is a standalone **COOLJAPAN Pure-Rust Policy §5 quarantine crate**. It wraps the third-party [`notify`](https://crates.io/crates/notify) file-watcher, which is not Pure Rust:

- on Linux, `notify` unconditionally pulls `inotify` → `inotify-sys` (C FFI)
- on macOS, `notify` pulls `fsevent-sys` (C FFI)
- on *BSD, `notify` pulls `kqueue-sys` (C FFI)

`notify` offers no build configuration that drops these OS file-watching FFI backends (`default-features = false` does **not** remove the Linux inotify backend), so per policy §5 the impurity is quarantined here rather than allowed to leak into `oxiui-compute-wgpu`. Apps that want live WGSL shader hot-reload depend on this crate directly and construct `ShaderWatcher::new()` themselves; `oxiui-compute-wgpu` itself stays 100% Pure Rust.

Unlike other quarantine crates in the workspace, `oxiui-hot-reload-notify` has no feature-flag toggle — `notify` is a direct, always-on dependency, since watching files is this crate's sole purpose.

## Installation

```toml
[dependencies]
oxiui-hot-reload-notify = "0.2.3"
```

## Quick Start

```rust,no_run
use std::path::PathBuf;
use oxiui_hot_reload_notify::ShaderWatcher;

let mut watcher = ShaderWatcher::new();
let shader_path = PathBuf::from("shaders/my_kernel.wgsl");
watcher.watch(&shader_path).expect("path must exist");

// In the render loop:
let changed: Vec<PathBuf> = watcher.drain_changed();
for path in changed {
    // Re-read the source and recompile the affected pipeline.
    let _src = std::fs::read_to_string(&path);
}
```

## API Overview

| Item | Description |
|------|-------------|
| `ShaderWatcher` | `new()` (panics on watcher-init failure), `try_new() -> Result<Self, ShaderWatchError>`, `watch(&mut self, path: &Path) -> Result<(), ShaderWatchError>`, `unwatch(&mut self, path: &Path) -> bool`, `drain_changed(&self) -> Vec<PathBuf>`, `watched_count(&self) -> usize`, `is_empty(&self) -> bool`. Implements `Default`. |
| `ShaderWatchError` | `Notify(String)` / `Io(String)`. Implements `Display` and `std::error::Error`. |

`watch` requires the path to already exist and monitors it with `RecursiveMode::NonRecursive` (the file itself, not its parent directory tree). `drain_changed` clears the internal changed-set on each call and returns raw, possibly non-canonical event paths; callers should `canonicalize()` before comparing against known paths.

## Related crates

- [`oxiui-compute-wgpu`](https://crates.io/crates/oxiui-compute-wgpu) — the Pure-Rust WGSL compute crate this watcher is designed to feed (via `compute_pipeline` / `PipelineCache::get_or_compile`)

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
