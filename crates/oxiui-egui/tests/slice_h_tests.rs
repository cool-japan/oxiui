use oxiui_core::UiCtx;
use oxiui_egui::EguiUiCtx;

/// Run a closure against an `EguiUiCtx` in a headless egui frame.
///
/// Uses `Context::run_ui` (egui 0.34 non-deprecated form) so that clippy
/// does not flag `#[deprecated]` usage.
fn run_ui<F: FnMut(&mut EguiUiCtx<'_>)>(mut f: F) {
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        f(&mut oxi);
    });
}

#[test]
fn horizontal_no_panic() {
    run_ui(|ctx| {
        let r = ctx.horizontal(&mut |ui| {
            ui.label("a");
            ui.label("b");
        });
        let _ = r;
    });
}

#[test]
fn vertical_no_panic() {
    run_ui(|ctx| {
        let _ = ctx.vertical(&mut |ui| {
            ui.label("a");
        });
    });
}

#[test]
fn grid_no_panic() {
    run_ui(|ctx| {
        let _ = ctx.grid(2, &mut |ui| {
            ui.label("c1");
            ui.label("c2");
        });
    });
}

#[test]
fn menu_bar_no_panic() {
    run_ui(|ctx| {
        let _ = ctx.menu_bar(&mut |ui| {
            ui.button("File");
        });
    });
}

#[test]
fn rich_text_three_spans() {
    use oxiui_core::RichTextSpan;
    run_ui(|ctx| {
        let spans = vec![
            RichTextSpan::new("Hello").color([255, 0, 0, 255]),
            RichTextSpan::new(" ").font_size(12.0),
            RichTextSpan::new("World").italic(),
        ];
        let r = ctx.rich_text(&spans);
        let _ = r;
    });
}

#[test]
fn drag_source_no_panic() {
    run_ui(|ctx| {
        let _ = ctx.drag_source(42, &mut |ui| {
            ui.label("drag me");
        });
    });
}

#[test]
fn drop_target_no_panic() {
    run_ui(|ctx| {
        let _ = ctx.drop_target(&[42], &mut |ui| {
            ui.label("drop here");
        });
    });
}

#[test]
fn clipboard_set_get() {
    run_ui(|ctx| {
        ctx.clipboard_set("hello clipboard");
        // clipboard_get inspects the output command queue of the current frame.
        let _result = ctx.clipboard_get();
    });
}

#[test]
fn all_seven_compile_without_error() {
    // Linking this test binary verifies all 7 methods are implemented.
}

#[test]
fn horizontal_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.horizontal(&mut |ui| {
            ui.label("x");
        });
        assert!(r.supported);
    });
}

#[test]
fn vertical_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.vertical(&mut |ui| {
            ui.label("y");
        });
        assert!(r.supported);
    });
}

#[test]
fn grid_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.grid(3, &mut |ui| {
            ui.label("a");
            ui.label("b");
            ui.label("c");
        });
        assert!(r.supported);
    });
}

#[test]
fn menu_bar_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.menu_bar(&mut |ui| {
            ui.button("Edit");
        });
        assert!(r.supported);
    });
}

#[test]
fn rich_text_returns_supported() {
    use oxiui_core::RichTextSpan;
    run_ui(|ctx| {
        let spans = vec![RichTextSpan::new("test").bold()];
        let r = ctx.rich_text(&spans);
        assert!(r.supported);
    });
}

#[test]
fn drag_source_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.drag_source(99, &mut |ui| {
            ui.label("item");
        });
        assert!(r.supported);
    });
}

#[test]
fn drop_target_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.drop_target(&[1, 2, 3], &mut |ui| {
            ui.label("zone");
        });
        assert!(r.supported);
    });
}
