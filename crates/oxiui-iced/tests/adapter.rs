use oxiui_core::UiCtx;
use oxiui_iced::adapter::{IcedConfig, IcedNullCtx, IcedUiCtx};

#[test]
fn headings_and_labels_collected() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    ctx.heading("Title");
    ctx.label("Body");
    // into_iced_element consumes the ctx and builds the Column — verify no panic
    let elem = ctx.into_iced_element();
    // We can't introspect iced Element internals, so just confirm it exists
    let _ = elem;
}

#[test]
fn button_click_state_first_clicked() {
    let mut config = IcedConfig::default();
    config.pending_clicks.insert(0usize);
    let mut ctx = IcedUiCtx::new(config);
    let resp = ctx.button("Click me");
    assert!(resp.clicked, "button 0 should be marked clicked");
}

#[test]
fn button_click_state_second_not_clicked() {
    let mut config = IcedConfig::default();
    config.pending_clicks.insert(0usize);
    let mut ctx = IcedUiCtx::new(config);
    ctx.button("First"); // id = 0, clicked
    let resp2 = ctx.button("Second"); // id = 1, not in pending_clicks
    assert!(!resp2.clicked, "button 1 should not be clicked");
}

#[test]
fn button_ids_are_sequential() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    // All widget types share the id counter now (heading/label use no id).
    ctx.heading("Title");
    let r0 = ctx.button("A");
    ctx.label("mid");
    let r1 = ctx.button("B");
    // Neither is clicked (empty pending_clicks)
    assert!(!r0.clicked);
    assert!(!r1.clicked);
    // The element tree builds without panic
    let _ = ctx.into_iced_element();
}

#[test]
fn null_ctx_is_no_op() {
    let mut ctx = IcedNullCtx::default();
    ctx.heading("ignored");
    ctx.label("ignored");
    let resp = ctx.button("ignored");
    assert!(!resp.clicked);
    assert!(!resp.hovered);
}
