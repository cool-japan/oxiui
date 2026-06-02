//! Pluggable backend runner infrastructure for OxiUI.
//!
//! The [`BackendRunner`] trait decouples backend selection from [`crate::App::run`]
//! dispatch logic. Each backend provides its own [`BackendRunner`] implementation;
//! the app selects and boxes the appropriate runner at run time.
//!
//! # Current state
//! [`EguiRunner`] and `IcedRunner` are wiring stubs. The live rendering paths
//! remain in `lib.rs`'s `run_egui_or_fallback` / `run_iced` methods. These stubs
//! exist to provide a stable public trait surface; full delegation is planned for M6.

use crate::{AppConfig, AppExit};
use oxiui_core::UiError;

/// Content closure type passed to a backend runner.
///
/// A boxed, send-capable, frame-driven closure that receives a mutable reference
/// to a [`oxiui_core::UiCtx`] on each frame.
pub type ContentFn = Box<dyn FnMut(&mut dyn oxiui_core::UiCtx) + Send>;

/// Lifecycle callbacks passed to a [`BackendRunner`] at startup.
///
/// Backends call these closures when the corresponding window event fires.
/// Closures that are `None` are silently skipped.
#[derive(Default)]
pub struct LifecycleConfig {
    /// Called when the window close button is pressed or the process is asked to quit.
    pub on_close: Option<Box<dyn FnMut() + Send>>,
    /// Called when the window is resized; arguments are the new `(width, height)` in logical pixels.
    pub on_resize: Option<Box<dyn FnMut(f32, f32) + Send>>,
    /// Called when the window gains (`true`) or loses (`false`) focus.
    pub on_focus: Option<Box<dyn FnMut(bool) + Send>>,
}

/// Trait for pluggable backend runners.
///
/// Implement this trait to integrate a new GUI backend with OxiUI. The trait is
/// object-safe; callers box the implementor and invoke [`BackendRunner::run`].
pub trait BackendRunner: Send + 'static {
    /// Launch the backend event loop.
    ///
    /// This call blocks until the event loop terminates, then returns the
    /// exit status or a [`UiError`] describing why the backend failed to start.
    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError>;
}

/// [`BackendRunner`] stub for the egui backend.
///
/// The live rendering path for egui remains in `App::run_egui_or_fallback`.
/// This stub is provided as a stable public type for dependency injection
/// and testing; it returns `Ok(AppExit::RequestedByUser)` immediately.
#[cfg(feature = "egui")]
pub struct EguiRunner;

#[cfg(feature = "egui")]
impl BackendRunner for EguiRunner {
    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        _lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError> {
        // Stub: the live egui run path is in lib.rs `run_egui_or_fallback`.
        // Full delegation to this runner is planned for M6.
        let _ = (config, content);
        Ok(AppExit::RequestedByUser)
    }
}

/// [`BackendRunner`] stub for the iced backend.
///
/// The live rendering path for iced remains in `App::run_iced`.
/// This stub is provided as a stable public type for dependency injection
/// and testing; it returns `Ok(AppExit::RequestedByUser)` immediately.
#[cfg(feature = "iced")]
pub struct IcedRunner;

#[cfg(feature = "iced")]
impl BackendRunner for IcedRunner {
    fn run(
        self: Box<Self>,
        config: AppConfig,
        content: ContentFn,
        _lifecycle: LifecycleConfig,
    ) -> Result<AppExit, UiError> {
        // Stub: the live iced run path is in lib.rs `run_iced`.
        // Full delegation to this runner is planned for M6.
        let _ = (config, content);
        Ok(AppExit::RequestedByUser)
    }
}
