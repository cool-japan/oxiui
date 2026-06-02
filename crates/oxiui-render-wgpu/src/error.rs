//! GPU error mapping to [`oxiui_core::UiError`].
//!
//! GPU backends produce hardware-specific error conditions.  This module
//! normalises them into the common [`oxiui_core::UiError`] type so callers
//! need only handle one error hierarchy.

use oxiui_core::UiError;

// ── GpuErrorKind ─────────────────────────────────────────────────────────────

/// The class of error reported by the GPU runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum GpuErrorKind {
    /// The GPU device was lost (driver crash, unplug, TDR, …).
    DeviceLost,
    /// The GPU ran out of memory.
    OutOfMemory,
    /// A shader module failed to compile.
    ShaderCompile,
    /// The swap-chain surface was lost (window closed, resize race, …).
    SurfaceLost,
}

/// Map a [`GpuErrorKind`] and a detail string to a [`UiError`].
///
/// | Kind | Maps to |
/// |------|---------|
/// | `DeviceLost` | `UiError::Render` |
/// | `OutOfMemory` | `UiError::Render` |
/// | `SurfaceLost` | `UiError::Render` |
/// | `ShaderCompile` | `UiError::Unsupported` |
pub fn map_gpu_error(kind: GpuErrorKind, detail: String) -> UiError {
    match kind {
        GpuErrorKind::DeviceLost => {
            UiError::Render(format!("GPU {:?}: {detail}", GpuErrorKind::DeviceLost))
        }
        GpuErrorKind::OutOfMemory => {
            UiError::Render(format!("GPU {:?}: {detail}", GpuErrorKind::OutOfMemory))
        }
        GpuErrorKind::SurfaceLost => {
            UiError::Render(format!("GPU {:?}: {detail}", GpuErrorKind::SurfaceLost))
        }
        GpuErrorKind::ShaderCompile => UiError::Unsupported(format!("shader compile: {detail}")),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_gpu_error_all_kinds() {
        let device_lost = map_gpu_error(GpuErrorKind::DeviceLost, "reset".to_string());
        assert!(
            matches!(device_lost, UiError::Render(ref s) if s.contains("DeviceLost")),
            "expected Render(DeviceLost …), got {device_lost:?}"
        );

        let oom = map_gpu_error(GpuErrorKind::OutOfMemory, "vram".to_string());
        assert!(
            matches!(oom, UiError::Render(ref s) if s.contains("OutOfMemory")),
            "expected Render(OutOfMemory …), got {oom:?}"
        );

        let surface = map_gpu_error(GpuErrorKind::SurfaceLost, "resize".to_string());
        assert!(
            matches!(surface, UiError::Render(ref s) if s.contains("SurfaceLost")),
            "expected Render(SurfaceLost …), got {surface:?}"
        );

        let shader = map_gpu_error(GpuErrorKind::ShaderCompile, "syntax error".to_string());
        assert!(
            matches!(shader, UiError::Unsupported(ref s) if s.contains("shader compile")),
            "expected Unsupported(shader compile …), got {shader:?}"
        );
    }
}
