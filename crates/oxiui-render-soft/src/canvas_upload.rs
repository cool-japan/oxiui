//! Canvas 2D pixel upload path — write a [`Framebuffer`] to an HTML canvas element.
//!
//! This module provides a `putImageData`-based upload path as an alternative
//! to WebGPU for environments that support Canvas 2D but not WebGPU (older
//! browsers, embedded environments, certain CI configurations).
//!
//! # Platform notes
//!
//! - **wasm32**: the `upload_framebuffer` and `upload_rgba` functions are fully
//!   implemented using `web_sys::CanvasRenderingContext2d` and `ImageData`.
//! - **native**: both functions are no-ops that always return `Ok(())`.
//!   This enables `--all-features` native builds to compile successfully.
//!
//! # Feature gate
//!
//! Enable the `canvas-2d` Cargo feature to activate this module.  Without that
//! feature, the module is still compiled but contains only the native stubs.
//!
//! # Example (wasm32, `canvas-2d` feature)
//!
//! ```rust,ignore
//! use oxiui_render_soft::{Framebuffer, canvas_upload::upload_framebuffer};
//! use oxiui_core::Color;
//!
//! let fb = Framebuffer::with_fill(320, 240, Color(30, 30, 30, 255));
//! upload_framebuffer(&fb, "my-canvas").expect("canvas upload failed");
//! ```

use crate::framebuffer::Framebuffer;

// ---------------------------------------------------------------------------
// wasm32 implementation
// ---------------------------------------------------------------------------

/// Upload the pixel contents of `fb` to the HTML canvas element with the
/// given `canvas_id` using `CanvasRenderingContext2d.putImageData`.
///
/// The canvas is resized to `fb.width() × fb.height()` before the upload.
///
/// # Errors
///
/// Returns a descriptive `String` error if:
/// * The `canvas_id` element is not found in the DOM.
/// * The element is not an `<canvas>` element.
/// * The 2D rendering context cannot be obtained.
/// * `ImageData` construction fails (browser restriction or out-of-memory).
#[cfg(all(feature = "canvas-2d", target_arch = "wasm32"))]
pub fn upload_framebuffer(fb: &Framebuffer, canvas_id: &str) -> Result<(), String> {
    let rgba_buf = framebuffer_to_rgba8(fb);
    upload_rgba(&rgba_buf, fb.width(), fb.height(), canvas_id)
}

/// Upload a raw RGBA8 byte slice to the given canvas element.
///
/// `data` must be exactly `width * height * 4` bytes in RGBA row-major order.
///
/// # Errors
///
/// Returns a descriptive error string on failure (see [`upload_framebuffer`]).
#[cfg(all(feature = "canvas-2d", target_arch = "wasm32"))]
pub fn upload_rgba(data: &[u8], width: u32, height: u32, canvas_id: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let window =
        web_sys::window().ok_or_else(|| "canvas_upload: no global window object".to_owned())?;
    let document = window
        .document()
        .ok_or_else(|| "canvas_upload: window.document is None".to_owned())?;

    // Locate the canvas element.
    let element = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| format!("canvas_upload: element '{canvas_id}' not found"))?;
    let canvas: web_sys::HtmlCanvasElement = element
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("canvas_upload: '{canvas_id}' is not an <canvas>"))?;

    // Resize the canvas to match the framebuffer.
    canvas.set_width(width);
    canvas.set_height(height);

    // Obtain 2D context.
    let ctx = canvas
        .get_context("2d")
        .map_err(|e| format!("canvas_upload: get_context('2d') failed: {e:?}"))?
        .ok_or_else(|| format!("canvas_upload: '2d' context not available on '{canvas_id}'"))?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "canvas_upload: context is not CanvasRenderingContext2d".to_owned())?;

    // Build ImageData from the RGBA8 byte slice.
    // `ImageData::new_with_u8_clamped_array_and_sh` creates an ImageData from a
    // `Uint8ClampedArray`.  The slice is copied into Wasm linear memory via
    // `js_sys::Uint8ClampedArray::view` (zero-copy borrow).
    //
    // Safety: `view` is sound here because we do not invoke any JS that could
    // cause the Wasm heap to grow between `view()` and `new_with_u8_clamped_array_and_sh()`.
    let clamped_arr = js_sys::Uint8ClampedArray::from(data);
    let image_data =
        web_sys::ImageData::new_with_u8_clamped_array_and_sh(&clamped_arr, width, height)
            .map_err(|e| format!("canvas_upload: ImageData construction failed: {e:?}"))?;

    // Upload.
    ctx.put_image_data(&image_data, 0.0, 0.0)
        .map_err(|e| format!("canvas_upload: putImageData failed: {e:?}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Native stubs
// ---------------------------------------------------------------------------

/// Native-platform stub — always returns `Ok(())`.
///
/// On `wasm32` + `canvas-2d` the real implementation is active.
#[cfg(not(all(feature = "canvas-2d", target_arch = "wasm32")))]
#[allow(unused_variables)]
pub fn upload_framebuffer(fb: &Framebuffer, canvas_id: &str) -> Result<(), String> {
    Ok(())
}

/// Native-platform stub — always returns `Ok(())`.
///
/// On `wasm32` + `canvas-2d` the real implementation is active.
#[cfg(not(all(feature = "canvas-2d", target_arch = "wasm32")))]
#[allow(unused_variables)]
pub fn upload_rgba(data: &[u8], width: u32, height: u32, canvas_id: &str) -> Result<(), String> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Convert a [`Framebuffer`] (`0xAARRGGBB`) to a flat RGBA8 byte vector
/// (`[R, G, B, A, ...]`) as expected by `ImageData`.
pub fn framebuffer_to_rgba8(fb: &Framebuffer) -> Vec<u8> {
    use crate::framebuffer::unpack;
    let mut out = Vec::with_capacity(fb.width() as usize * fb.height() as usize * 4);
    for &px in fb.pixels() {
        let (r, g, b, a) = unpack(px);
        out.extend_from_slice(&[r, g, b, a]);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::Color;

    #[test]
    fn framebuffer_to_rgba8_correct_layout() {
        let fb = Framebuffer::with_fill(2, 1, Color(10, 20, 30, 200));
        let raw = framebuffer_to_rgba8(&fb);
        assert_eq!(raw.len(), 8);
        // First pixel: R=10, G=20, B=30, A=200
        assert_eq!(&raw[..4], &[10, 20, 30, 200]);
        // Second pixel: same
        assert_eq!(&raw[4..], &[10, 20, 30, 200]);
    }

    #[test]
    fn framebuffer_to_rgba8_transparent() {
        let fb = Framebuffer::new(1, 1); // transparent black
        let raw = framebuffer_to_rgba8(&fb);
        assert_eq!(raw, vec![0, 0, 0, 0]);
    }

    #[test]
    fn upload_framebuffer_native_noop() {
        // On native, both upload functions are no-ops and return Ok.
        let fb = Framebuffer::with_fill(4, 4, Color(255, 0, 0, 255));
        let result = upload_framebuffer(&fb, "test-canvas");
        assert!(result.is_ok(), "native stub must return Ok: {result:?}");
    }

    #[test]
    fn upload_rgba_native_noop() {
        // 4×4 red RGBA: 16 pixels × 4 channels = 64 bytes
        let data: Vec<u8> = (0..64)
            .map(|i| match i % 4 {
                0 | 3 => 255,
                _ => 0,
            })
            .collect();
        let result = upload_rgba(&data, 4, 4, "test-canvas");
        assert!(result.is_ok(), "native stub must return Ok: {result:?}");
    }
}
