use oxiui_core::Color;
use oxiui_render_soft::{
    render_headless_once, render_headless_scene, Framebuffer, LinearGradient, SoftRenderer,
};

// ── Existing SoftRenderer tests ─────────────────────────────────────────────

#[test]
fn soft_renderer_constructs() {
    let _r = SoftRenderer::default();
}

#[test]
fn clear_frame_correct_length() {
    let r = SoftRenderer::default();
    let frame = r
        .clear_frame(4, 4, Color(0, 0, 0, 255))
        .expect("clear failed");
    assert_eq!(frame.len(), 16);
}

#[test]
fn clear_frame_zero_dimensions() {
    let r = SoftRenderer::default();
    let frame = r
        .clear_frame(0, 0, Color(255, 0, 0, 255))
        .expect("clear zero-size failed");
    assert_eq!(frame.len(), 0);
}

#[test]
fn clear_frame_large_buffer() {
    let r = SoftRenderer::default();
    let frame = r
        .clear_frame(100, 80, Color(122, 162, 247, 255))
        .expect("clear failed");
    assert_eq!(frame.len(), 8000);
}

// ── Headless RgbaBuffer tests ────────────────────────────────────────────────

#[test]
fn headless_buffer_dimensions() {
    let buf = render_headless_once(800, 600);
    assert_eq!(buf.width, 800);
    assert_eq!(buf.height, 600);
    assert_eq!(buf.data.len(), 800 * 600 * 4);
}

#[test]
fn headless_buffer_has_content() {
    let buf = render_headless_once(800, 600);
    assert!(
        buf.has_content(),
        "headless buffer must not be fully transparent/zero"
    );
}

#[test]
fn headless_buffer_zero_dimensions() {
    let buf = render_headless_once(0, 0);
    assert_eq!(buf.data.len(), 0);
    // has_content() is false for an empty buffer (nothing to check)
    assert!(!buf.has_content());
}

#[test]
fn headless_buffer_single_pixel() {
    let buf = render_headless_once(1, 1);
    assert_eq!(buf.data.len(), 4);
    assert!(buf.has_content());
}

#[test]
fn headless_buffer_bg_is_tokyo_night_dark() {
    let buf = render_headless_once(2, 2);
    // HEADLESS_BG_COLOR = [26, 27, 38, 255] (Tokyo Night #1A1B26)
    assert_eq!(buf.data[0], 26, "R should be 26 (#1A)");
    assert_eq!(buf.data[1], 27, "G should be 27 (#1B)");
    assert_eq!(buf.data[2], 38, "B should be 38 (#26)");
    assert_eq!(buf.data[3], 255, "A should be 255 (fully opaque)");
}

#[test]
fn headless_png_roundtrip() {
    let buf = render_headless_once(16, 8);
    let tmp = std::env::temp_dir().join("oxiui_headless_test.png");

    buf.save_png(&tmp).expect("PNG save failed");

    // Read back with the png decoder and verify dimensions
    let file = std::fs::File::open(&tmp).expect("could not open saved PNG");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let reader = decoder.read_info().expect("png read_info failed");
    let info = reader.info();
    assert_eq!(info.width, 16, "PNG width mismatch");
    assert_eq!(info.height, 8, "PNG height mismatch");
    assert_eq!(info.color_type, png::ColorType::Rgba);

    // Clean up
    let _ = std::fs::remove_file(&tmp);
}

// ── Scene rendering + rasterizer integration ────────────────────────────────

#[test]
fn clear_frame_actually_fills_color() {
    // Regression: clear_frame must fill with the requested colour, not zeros.
    let r = SoftRenderer::default();
    let frame = r
        .clear_frame(2, 2, Color(122, 162, 247, 255))
        .expect("clear");
    // 0xAARRGGBB for (122,162,247,255) = 0xFF7AA2F7.
    assert!(
        frame.iter().all(|&px| px == 0xFF7AA2F7),
        "all pixels filled"
    );
}

#[test]
fn render_scene_draws_rect() {
    let r = SoftRenderer::default();
    let fb = r.render(20, 20, Color(0, 0, 0, 255), |c| {
        c.fill_rect(5.0, 5.0, 10.0, 10.0, Color(255, 0, 0, 255));
    });
    assert_eq!(fb.get_rgba(10, 10), Some((255, 0, 0, 255)));
    assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
}

#[test]
fn headless_scene_has_drawn_content() {
    let buf = render_headless_scene(32, 32, |c| {
        c.fill_circle(16.0, 16.0, 8.0, Color(255, 255, 255, 255));
    });
    assert_eq!(buf.width, 32);
    assert!(buf.has_content());
    // Centre pixel index = (16*32 + 16) * 4.
    let idx = ((16 * 32 + 16) * 4) as usize;
    assert_eq!(buf.data[idx], 255, "circle centre should be white");
}

#[test]
fn gradient_fill_then_png_roundtrip_preserves_pixels() {
    let mut fb = Framebuffer::with_fill(8, 1, Color(0, 0, 0, 255));
    let clip = oxiui_render_soft::ClipRect::full(8, 1);
    let g = LinearGradient::two_stop(
        (0.0, 0.0),
        (8.0, 0.0),
        Color(255, 0, 0, 255),
        Color(0, 0, 255, 255),
    );
    g.fill_rect(&mut fb, &clip, 0.0, 0.0, 8.0, 1.0);
    let before = fb.to_rgba_buffer();

    let tmp = std::env::temp_dir().join("oxiui_gradient_roundtrip.png");
    before.save_png(&tmp).expect("save png");

    let file = std::fs::File::open(&tmp).expect("open png");
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("read_info");
    let out_size = reader.output_buffer_size().expect("output buffer size");
    let mut decoded = vec![0u8; out_size];
    let frame = reader.next_frame(&mut decoded).expect("next_frame");
    decoded.truncate(frame.buffer_size());

    assert_eq!(decoded, before.data, "PNG round-trip must preserve pixels");
    let _ = std::fs::remove_file(&tmp);
}
