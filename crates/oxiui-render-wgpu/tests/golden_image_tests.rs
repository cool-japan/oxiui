//! Snapshot / golden-image tests for [`WgpuBackend`].
//!
//! Each test renders a reference scene into a headless backend, then compares
//! the pixel readback against a saved PNG baseline in the `testdata/golden/`
//! directory.
//!
//! # Baseline generation
//!
//! Set the environment variable `OXIUI_UPDATE_GOLDENS=1` before running the
//! tests to regenerate baselines from the current render output:
//!
//! ```sh
//! OXIUI_UPDATE_GOLDENS=1 cargo nextest run -p oxiui-render-wgpu golden
//! ```
//!
//! # Pixel-difference threshold
//!
//! Each test allows up to [`MAX_PIXEL_DIFF`] average absolute difference per
//! channel per pixel.  This accounts for minor GPU/driver numerical differences
//! across platforms (Metal on ARM may differ by 1 LSB from software reference).

use std::io::BufReader;
use std::path::{Path, PathBuf};

use oxiui_core::geometry::{Point, Rect};
use oxiui_core::paint::{GradientStop, ImageFilter, RenderBackend};
use oxiui_core::{Color, DrawList, UiError};
use oxiui_render_wgpu::WgpuBackend;

// Maximum tolerated mean per-pixel per-channel absolute difference.
// A value of 2.0 / 255 ≈ 0.008 allows for small GPU rounding differences.
const MAX_PIXEL_DIFF: f64 = 2.0;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn try_backend(w: u32, h: u32) -> Option<WgpuBackend> {
    match WgpuBackend::headless(w, h) {
        Ok(b) => Some(b),
        Err(UiError::Unsupported(msg)) => {
            println!("skip: no GPU adapter ({msg})");
            None
        }
        Err(e) => panic!("unexpected backend init error: {e}"),
    }
}

fn golden_dir() -> PathBuf {
    // Place goldens relative to the test file's directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
        .join("golden")
}

fn golden_path(name: &str) -> PathBuf {
    golden_dir().join(format!("{name}.png"))
}

/// Save RGBA bytes as a PNG file.
fn save_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    use std::fs::File;
    use std::io::BufWriter;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create golden dir");
    }
    let file = File::create(path).expect("create golden PNG");
    let w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(rgba).expect("write PNG data");
}

/// Load a PNG file and return its RGBA bytes, width, and height.
fn load_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("failed to open golden PNG {path:?}: {e}"));
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder.read_info().expect("read PNG info");
    let buf_size = reader.output_buffer_size().expect("PNG buffer size");
    let mut buf = vec![0u8; buf_size];
    let info = reader.next_frame(&mut buf).expect("read PNG frame");
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

/// Compare two RGBA buffers of the same dimensions.
/// Returns the mean absolute per-channel per-pixel difference.
fn mean_pixel_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(
        a.len(),
        b.len(),
        "pixel buffers must have the same size for comparison"
    );
    let total: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&av, &bv)| (av as i32 - bv as i32).unsigned_abs() as u64)
        .sum();
    total as f64 / a.len() as f64
}

/// Render a scene, optionally save / compare against a golden PNG.
fn assert_golden(name: &str, backend: &WgpuBackend, width: u32, height: u32) {
    let rendered = backend.readback_rgba().expect("readback for golden");

    let update = std::env::var("OXIUI_UPDATE_GOLDENS").is_ok_and(|v| v == "1");
    let path = golden_path(name);

    if update || !path.exists() {
        save_png(&path, &rendered, width, height);
        println!("[golden] saved: {path:?}");
        return;
    }

    let (baseline, bw, bh) = load_png(&path);
    assert_eq!(
        (bw, bh),
        (width, height),
        "golden dimensions mismatch for '{name}': expected {width}×{height}, got {bw}×{bh}"
    );
    let diff = mean_pixel_diff(&rendered, &baseline);
    assert!(
        diff <= MAX_PIXEL_DIFF,
        "golden '{name}' pixel diff {diff:.3} exceeds threshold {MAX_PIXEL_DIFF}"
    );
}

// ── Reference scenes ──────────────────────────────────────────────────────────

fn scene_solid_rects(list: &mut DrawList) {
    list.push_rect(Rect::new(10.0, 10.0, 60.0, 60.0), Color(200, 50, 50, 255));
    list.push_rect(Rect::new(80.0, 10.0, 60.0, 60.0), Color(50, 200, 50, 255));
    list.push_rect(Rect::new(10.0, 80.0, 60.0, 60.0), Color(50, 50, 200, 255));
    list.push_rect(Rect::new(80.0, 80.0, 60.0, 60.0), Color(200, 200, 50, 255));
}

fn scene_rounded_rects(list: &mut DrawList) {
    list.push_rounded_rect(
        Rect::new(10.0, 10.0, 80.0, 80.0),
        12.0,
        Color(180, 80, 80, 255),
    );
    list.push_rounded_rect(
        Rect::new(110.0, 10.0, 80.0, 80.0),
        4.0,
        Color(80, 180, 80, 255),
    );
}

fn scene_circles_and_ellipses(list: &mut DrawList) {
    list.push_circle(Point::new(50.0, 50.0), 40.0, Color(100, 200, 255, 255));
    list.push_ellipse(
        Point::new(150.0, 50.0),
        50.0,
        30.0,
        Color(255, 150, 100, 255),
    );
}

fn scene_gradient(list: &mut DrawList) {
    list.push(oxiui_core::DrawCommand::LinearGradient {
        rect: Rect::new(0.0, 0.0, 200.0, 100.0),
        start: Point::new(0.0, 50.0),
        end: Point::new(200.0, 50.0),
        stops: vec![
            GradientStop::new(0.0, Color(255, 0, 0, 255)),
            GradientStop::new(0.5, Color(0, 255, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 255, 255)),
        ],
    });
}

fn scene_image_blit(list: &mut DrawList) {
    // 4×4 checkerboard (red / blue).
    let mut rgba = vec![0u8; 4 * 4 * 4];
    for y in 0..4u32 {
        for x in 0..4u32 {
            let i = ((y * 4 + x) * 4) as usize;
            if (x + y) % 2 == 0 {
                rgba[i] = 255;
                rgba[i + 3] = 255; // red
            } else {
                rgba[i + 2] = 255;
                rgba[i + 3] = 255; // blue
            }
        }
    }
    list.push_image(
        oxiui_core::paint::ImageData::new(rgba, 4, 4),
        Rect::new(10.0, 10.0, 100.0, 100.0),
        ImageFilter::Nearest,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn golden_solid_rects() {
    let w = 160u32;
    let h = 160u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    scene_solid_rects(&mut list);
    b.execute(&list).expect("execute");
    assert_golden("solid_rects", &b, w, h);
}

#[test]
fn golden_rounded_rects() {
    let w = 200u32;
    let h = 120u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    scene_rounded_rects(&mut list);
    b.execute(&list).expect("execute");
    assert_golden("rounded_rects", &b, w, h);
}

#[test]
fn golden_circles_and_ellipses() {
    let w = 200u32;
    let h = 100u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    scene_circles_and_ellipses(&mut list);
    b.execute(&list).expect("execute");
    assert_golden("circles_and_ellipses", &b, w, h);
}

#[test]
fn golden_gradient() {
    let w = 200u32;
    let h = 100u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    scene_gradient(&mut list);
    b.execute(&list).expect("execute");
    assert_golden("gradient_linear", &b, w, h);
}

#[test]
fn golden_image_blit() {
    let w = 120u32;
    let h = 120u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    scene_image_blit(&mut list);
    b.execute(&list).expect("execute");
    assert_golden("image_blit", &b, w, h);
}

/// Composite scene: solid rect clipped to a small region, with a gradient below.
#[test]
fn golden_clip_and_gradient() {
    let w = 160u32;
    let h = 160u32;
    let Some(mut b) = try_backend(w, h) else {
        return;
    };
    let mut list = DrawList::new();
    // Background gradient.
    list.push(oxiui_core::DrawCommand::LinearGradient {
        rect: Rect::new(0.0, 0.0, 160.0, 160.0),
        start: Point::new(0.0, 0.0),
        end: Point::new(160.0, 160.0),
        stops: vec![
            GradientStop::new(0.0, Color(240, 240, 240, 255)),
            GradientStop::new(1.0, Color(100, 100, 200, 255)),
        ],
    });
    // Clipped solid rect.
    list.push_clip(Rect::new(20.0, 20.0, 120.0, 120.0));
    list.push_rect(Rect::new(0.0, 0.0, 160.0, 160.0), Color(255, 100, 50, 200));
    list.pop_clip();
    b.execute(&list).expect("execute");
    assert_golden("clip_and_gradient", &b, w, h);
}
