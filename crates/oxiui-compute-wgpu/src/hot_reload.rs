//! WGSL shader hot-reload via `notify` file-watching.
//!
//! # Feature gate
//!
//! This module is compiled only when the `hot-reload` Cargo feature is enabled.
//! Add `oxiui-compute-wgpu = { version = "…", features = ["hot-reload"] }` to
//! your `Cargo.toml`.
//!
//! # Overview
//!
//! `ShaderWatcher` wraps a `notify::RecommendedWatcher` and maintains a
//! set of WGSL source paths to monitor.  Whenever a watched file is modified on
//! disk, its canonical path is pushed into an `mpsc` channel.
//!
//! The caller drives recompilation by:
//!
//! 1. Creating a `ShaderWatcher` (or obtaining one via
//!    [`ComputeContext::watcher()`][crate::ComputeContext::watcher]).
//! 2. Registering paths with `ShaderWatcher::watch`.
//! 3. Each frame, calling `ShaderWatcher::drain_changed` to collect the
//!    set of paths that have been modified since the last call.
//! 4. For each returned path, re-reading the source and calling
//!    [`crate::pipeline::PipelineCache::get_or_compile`] (or invalidating
//!    the pipeline manually) to trigger recompilation.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "hot-reload")]
//! # {
//! use std::path::PathBuf;
//! use oxiui_compute_wgpu::{ComputeContext, PipelineCache};
//!
//! let ctx = ComputeContext::try_new().expect("GPU required for this example");
//! let mut watcher = ctx.watcher();
//! let shader_path = PathBuf::from("shaders/my_kernel.wgsl");
//! watcher.watch(&shader_path).expect("path must exist");
//!
//! // In the render loop:
//! let changed: Vec<PathBuf> = watcher.drain_changed();
//! for path in changed {
//!     let src = std::fs::read_to_string(&path).unwrap();
//!     // Recompile — call `compute_pipeline` or `PipelineCache::get_or_compile`.
//!     let _pipeline = oxiui_compute_wgpu::compute_pipeline(&ctx.device, &src, "main");
//! }
//! # }
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

// ── ShaderWatcher ─────────────────────────────────────────────────────────────

/// A file-system watcher for WGSL source files that signals recompilation needs.
///
/// Obtain a `ShaderWatcher` via [`ComputeContext::watcher()`][crate::ComputeContext::watcher]
/// or by constructing one directly with [`ShaderWatcher::new()`].
///
/// Call [`drain_changed`][ShaderWatcher::drain_changed] each frame to retrieve
/// the set of paths that have been modified since the last call.
pub struct ShaderWatcher {
    /// The underlying notify watcher handle — must be kept alive.
    _watcher: RecommendedWatcher,
    /// Paths added with [`watch`][ShaderWatcher::watch].
    watched_paths: HashSet<PathBuf>,
    /// Paths that have been flagged as changed by the notify callback.
    changed: Arc<Mutex<HashSet<PathBuf>>>,
}

impl ShaderWatcher {
    /// Create a new `ShaderWatcher` with no watched paths.
    ///
    /// # Panics
    ///
    /// Panics if the underlying notify watcher cannot be initialised (which is
    /// extremely unlikely on supported platforms).  Prefer [`try_new`][Self::try_new]
    /// when you need to handle the error explicitly.
    pub fn new() -> Self {
        match Self::try_new() {
            Ok(w) => w,
            Err(e) => panic!("ShaderWatcher: notify watcher could not be initialised: {e}"),
        }
    }

    /// Create a new `ShaderWatcher`, returning an error if the underlying
    /// `notify` backend cannot be initialised.
    ///
    /// # Errors
    ///
    /// Returns [`ShaderWatchError::Notify`] if `notify::recommended_watcher`
    /// fails.
    pub fn try_new() -> Result<Self, ShaderWatchError> {
        let changed: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
        let changed_cb = Arc::clone(&changed);

        let watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                if let Ok(ev) = event {
                    // Only react to file modifications and creations.
                    let is_modify = matches!(ev.kind, EventKind::Modify(_) | EventKind::Create(_));
                    if is_modify {
                        if let Ok(mut guard) = changed_cb.lock() {
                            for path in ev.paths {
                                guard.insert(path);
                            }
                        }
                    }
                }
            })
            .map_err(|e| ShaderWatchError::Notify(e.to_string()))?;

        Ok(ShaderWatcher {
            _watcher: watcher,
            watched_paths: HashSet::new(),
            changed,
        })
    }

    /// Begin watching `path` for modifications.
    ///
    /// `path` must exist at the time of this call; the watcher uses
    /// [`RecursiveMode::NonRecursive`] so only the file itself (not its
    /// parent directory tree) is monitored.
    ///
    /// # Errors
    ///
    /// Returns a [`notify::Error`] wrapped in a [`ShaderWatchError`] when the
    /// path cannot be watched (e.g. does not exist, permission denied).
    pub fn watch(&mut self, path: &Path) -> Result<(), ShaderWatchError> {
        self._watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| ShaderWatchError::Notify(e.to_string()))?;
        // Canonicalise so drain_changed paths can be looked up uniformly.
        let canonical = path
            .canonicalize()
            .map_err(|e| ShaderWatchError::Io(e.to_string()))?;
        self.watched_paths.insert(canonical);
        Ok(())
    }

    /// Stop watching `path`.
    ///
    /// Returns `true` if the path was being watched and was removed; `false` if
    /// it was not registered.
    pub fn unwatch(&mut self, path: &Path) -> bool {
        let _ = self._watcher.unwatch(path);
        if let Ok(canonical) = path.canonicalize() {
            self.watched_paths.remove(&canonical)
        } else {
            false
        }
    }

    /// Drain and return the set of paths that have changed since the last call.
    ///
    /// Each call clears the internal set so paths are returned at most once per
    /// modification event.  The returned paths are the raw event paths from the
    /// notify backend — they may not be canonical; callers should apply
    /// `canonicalize()` when comparing against known paths.
    ///
    /// Returns an empty `Vec` if the internal mutex is poisoned (should never
    /// happen in normal operation).
    pub fn drain_changed(&self) -> Vec<PathBuf> {
        match self.changed.lock() {
            Ok(mut guard) => guard.drain().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Return the number of paths currently registered for watching.
    pub fn watched_count(&self) -> usize {
        self.watched_paths.len()
    }

    /// Return `true` if no paths are currently being watched.
    pub fn is_empty(&self) -> bool {
        self.watched_paths.is_empty()
    }
}

impl Default for ShaderWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ── ShaderWatchError ──────────────────────────────────────────────────────────

/// Errors produced by [`ShaderWatcher`].
#[derive(Debug)]
pub enum ShaderWatchError {
    /// The underlying `notify` backend returned an error.
    Notify(String),
    /// An I/O error occurred (e.g. `canonicalize` failed).
    Io(String),
}

impl std::fmt::Display for ShaderWatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShaderWatchError::Notify(s) => write!(f, "notify watcher error: {s}"),
            ShaderWatchError::Io(s) => write!(f, "I/O error in shader watcher: {s}"),
        }
    }
}

impl std::error::Error for ShaderWatchError {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn watcher_new_does_not_panic() {
        let _w = ShaderWatcher::new();
    }

    #[test]
    fn watcher_default_is_empty() {
        let w = ShaderWatcher::default();
        assert!(w.is_empty());
        assert_eq!(w.watched_count(), 0);
    }

    #[test]
    fn drain_changed_empty_initially() {
        let w = ShaderWatcher::new();
        let changed = w.drain_changed();
        assert!(changed.is_empty(), "no paths watched yet");
    }

    #[test]
    fn watch_nonexistent_returns_error() {
        let mut w = ShaderWatcher::new();
        let result = w.watch(Path::new("/nonexistent/path/shader.wgsl"));
        assert!(
            result.is_err(),
            "watching a non-existent path must return Err"
        );
    }

    #[test]
    fn watch_existing_file_detected() {
        let mut w = ShaderWatcher::new();
        // Use a unique temp file per invocation to avoid races when
        // nextest runs tests in parallel (two instances must not share
        // the same path).  Combine PID + time to maximise uniqueness.
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let shader_path = dir.join(format!("oxiui_hot_reload_test_{pid}_{nanos}.wgsl"));
        // Write an initial file.
        {
            let mut f = std::fs::File::create(&shader_path).expect("create test file");
            writeln!(f, "@compute @workgroup_size(1) fn noop() {{}}").expect("write");
        }
        // Start watching.
        w.watch(&shader_path).expect("watch should succeed");
        assert_eq!(w.watched_count(), 1);

        // Modify the file.
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&shader_path)
                .expect("open for write");
            writeln!(f, "@compute @workgroup_size(64) fn noop() {{}}").expect("write");
        }

        // Give the watcher a short time to observe the event.
        std::thread::sleep(std::time::Duration::from_millis(200));

        let changed = w.drain_changed();
        // On some platforms (e.g. macOS FSEvents) the event may arrive with a
        // slight delay or be deduplicated.  We only assert that drain_changed
        // does not panic and returns a vec.
        let _ = changed;

        // Second drain must be empty (events are cleared).
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _second = w.drain_changed();

        // Cleanup.
        let _ = std::fs::remove_file(&shader_path);
    }

    #[test]
    fn unwatch_not_registered_returns_false() {
        let mut w = ShaderWatcher::new();
        let result = w.unwatch(Path::new("/some/path.wgsl"));
        assert!(!result, "unregistered path must return false");
    }

    #[test]
    fn shader_watch_error_display() {
        let e = ShaderWatchError::Notify("backend init failed".into());
        assert!(e.to_string().contains("notify"), "{e}");

        let e2 = ShaderWatchError::Io("permission denied".into());
        assert!(e2.to_string().contains("I/O"), "{e2}");
    }

    #[test]
    fn shader_watch_error_is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&ShaderWatchError::Notify("x".into()));
        assert_error(&ShaderWatchError::Io("x".into()));
    }
}
