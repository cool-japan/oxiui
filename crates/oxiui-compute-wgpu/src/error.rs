//! Error types for GPU compute operations.
//!
//! [`ComputeError`] is the single error type returned by every fallible
//! operation in this crate.  No external `thiserror` dependency is used;
//! [`std::fmt::Display`] and [`std::error::Error`] are implemented manually to
//! keep the crate dependency-minimal.

// ── ComputeError ──────────────────────────────────────────────────────────────

/// Errors that can occur while initialising or using a GPU compute context.
#[derive(Debug)]
pub enum ComputeError {
    /// No GPU adapter was found on this host (headless CI, VM without GPU, …).
    ///
    /// Callers should treat this as a *graceful skip*, not a hard failure.
    NoAdapter,

    /// The adapter was found but the device/queue request failed.
    ///
    /// The contained string carries the underlying wgpu diagnostic.
    DeviceRequest(String),

    /// A GPU memory allocation failed or the device reported an out-of-memory
    /// condition via an error scope.
    OutOfMemory,

    /// WGSL shader compilation failed.
    ///
    /// The contained string carries the diagnostic message including line and
    /// column numbers as reported by `wgpu::CompilationInfo`.
    ShaderCompilation(String),

    /// A compute operation failed with structured context.
    ///
    /// `op` names the failing operation (e.g. `"read_back"`, `"dispatch"`);
    /// `detail` carries the parameters or wgpu-level message.
    Operation {
        /// The name of the failing operation.
        op: &'static str,
        /// Human-readable detail: parameters, buffer labels, entry-point names, etc.
        detail: String,
    },
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter => write!(f, "no suitable GPU adapter found"),
            Self::DeviceRequest(e) => write!(f, "device request failed: {e}"),
            Self::OutOfMemory => write!(f, "GPU out of memory"),
            Self::ShaderCompilation(msg) => write!(f, "WGSL compilation error: {msg}"),
            Self::Operation { op, detail } => {
                write!(f, "compute operation '{op}' failed: {detail}")
            }
        }
    }
}

impl std::error::Error for ComputeError {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_adapter_display() {
        let e = ComputeError::NoAdapter;
        let s = e.to_string();
        assert!(s.contains("no suitable GPU adapter"), "got: {s}");
    }

    #[test]
    fn device_request_display() {
        let e = ComputeError::DeviceRequest("timeout".to_string());
        let s = e.to_string();
        assert!(s.contains("device request failed"), "got: {s}");
        assert!(s.contains("timeout"), "got: {s}");
    }

    #[test]
    fn display_out_of_memory() {
        let e = ComputeError::OutOfMemory;
        let s = e.to_string();
        assert!(s.contains("out of memory"), "got: {s}");
    }

    #[test]
    fn display_shader_compilation() {
        let e = ComputeError::ShaderCompilation("line 3, col 5: unknown identifier".to_string());
        let s = e.to_string();
        assert!(s.contains("WGSL compilation error"), "got: {s}");
        assert!(s.contains("unknown identifier"), "got: {s}");
    }

    #[test]
    fn display_operation() {
        let e = ComputeError::Operation {
            op: "read_back",
            detail: "buffer label=staging size=4096".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("read_back"), "got: {s}");
        assert!(s.contains("staging"), "got: {s}");
    }

    #[test]
    fn compute_error_is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&ComputeError::NoAdapter);
        assert_error(&ComputeError::DeviceRequest("x".to_string()));
        assert_error(&ComputeError::OutOfMemory);
        assert_error(&ComputeError::ShaderCompilation("msg".to_string()));
        assert_error(&ComputeError::Operation {
            op: "dispatch",
            detail: "x".to_string(),
        });
    }
}
