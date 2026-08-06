//! Headless render smoke test — M5 gate.
//!
//! Renders a small scene entirely off-screen through the `software` feature's
//! pure-CPU rasterizer: no window, no GPU, and no display connection is
//! required. This is exactly the path `Dockerfile.ffi-audit` exercises to
//! prove OxiUI works inside a `rust:slim` container that has no windowing
//! system at all.
//!
//! The example exits non-zero (via `assert!`/`std::process::exit`) if the
//! rendered buffer does not contain the expected non-background content, so
//! it doubles as a CI/Docker smoke-test gate rather than a purely visual demo.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_headless -p oxiui --no-default-features --features software
//! ```

fn main() {
    let width = 64u32;
    let height = 48u32;

    // Draw a solid rectangle inset from the edges so the render is trivially
    // distinguishable from the plain background fill.
    let buf = oxiui::render::render_headless_scene(width, height, |canvas| {
        canvas.fill_rect(
            8.0,
            8.0,
            (width - 16) as f32,
            (height - 16) as f32,
            oxiui::Color(255, 64, 32, 255),
        );
    });

    assert_eq!(buf.width, width, "unexpected buffer width");
    assert_eq!(buf.height, height, "unexpected buffer height");
    assert_eq!(
        buf.data.len(),
        (width * height * 4) as usize,
        "unexpected buffer byte length"
    );

    if !buf.has_content() {
        eprintln!("FAIL — headless render produced an all-zero (blank) buffer");
        std::process::exit(1);
    }

    // Count pixels that differ from the COOLJAPAN dark background fill —
    // this is the "non-zero pixel count" acceptance gate M5 promised.
    let bg = oxiui_render_soft::headless::HEADLESS_BG_COLOR;
    let non_background_pixels = buf.data.chunks_exact(4).filter(|px| *px != bg).count();

    println!(
        "Rendered {width}x{height} headless frame: {non_background_pixels} non-background pixel(s) of {} total.",
        width * height
    );

    if non_background_pixels == 0 {
        eprintln!("FAIL — headless render drew nothing over the background fill");
        std::process::exit(1);
    }

    println!("OK — headless render path produced visible content with no window or GPU.");
}
