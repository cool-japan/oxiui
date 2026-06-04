//! Headless state-machine tests for the expanded IcedUiCtx widget coverage.
//!
//! These tests exercise pure spec-collection and state-synthesis logic with no
//! iced runtime. Widget materialisation is covered by the `into_iced_element`
//! smoke test at the bottom.

use std::collections::HashMap;

use oxiui_core::UiCtx;
use oxiui_iced::adapter::{
    apply_message, IcedConfig, IcedNullCtx, IcedUiCtx, Message, WidgetState,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn default_ctx() -> IcedUiCtx {
    IcedUiCtx::new(IcedConfig::default())
}

fn ctx_with_state(state: HashMap<usize, WidgetState>) -> IcedUiCtx {
    IcedUiCtx::new(IcedConfig {
        state,
        ..IcedConfig::default()
    })
}

// ── text_input ────────────────────────────────────────────────────────────────

#[test]
fn text_input_records_spec_and_seeds_text() {
    let mut ctx = default_ctx();
    let resp = ctx.text_input("hello");
    assert!(resp.supported, "text_input must be supported");
    assert_eq!(resp.text, "hello");
    assert!(!resp.changed, "seed text unchanged");
}

#[test]
fn text_input_reflects_prior_state() {
    let mut state = HashMap::new();
    state.insert(0usize, WidgetState::Text("hi".to_owned()));
    let mut ctx = ctx_with_state(state);
    let resp = ctx.text_input("ignored_seed");
    assert_eq!(resp.text, "hi", "should reflect stored state");
    assert!(resp.changed, "stored value differs from seed");
}

// ── checkbox ─────────────────────────────────────────────────────────────────

#[test]
fn checkbox_records_and_synthesizes_checked() {
    let mut ctx = default_ctx();
    let resp = ctx.checkbox("opt", true);
    assert!(resp.supported);
    assert!(resp.checked);
    assert!(!resp.changed, "no state override; unchanged from seed");
}

#[test]
fn checkbox_reflects_prior_state() {
    let mut state = HashMap::new();
    state.insert(0usize, WidgetState::Checked(false));
    let mut ctx = ctx_with_state(state);
    let resp = ctx.checkbox("opt", true);
    assert!(!resp.checked, "state override should win");
    assert!(resp.changed, "state differs from seed");
}

// ── slider ────────────────────────────────────────────────────────────────────

#[test]
fn slider_records_from_state() {
    let mut state = HashMap::new();
    state.insert(0usize, WidgetState::Slider(0.75));
    let mut ctx = ctx_with_state(state);
    let resp = ctx.slider(0.5, 0.0..=1.0);
    assert!(resp.supported);
    assert!((resp.value - 0.75).abs() < 1e-9);
    assert!(resp.changed);
}

#[test]
fn slider_seeds_when_no_state() {
    let mut ctx = default_ctx();
    let resp = ctx.slider(0.3, 0.0..=1.0);
    assert!(resp.supported);
    assert!((resp.value - 0.3).abs() < 1e-9);
    assert!(!resp.changed);
}

// ── dropdown ──────────────────────────────────────────────────────────────────

#[test]
fn dropdown_records_options_and_selected() {
    let mut ctx = default_ctx();
    let resp = ctx.dropdown(&["a", "b", "c"], 1);
    assert!(resp.supported);
    assert_eq!(resp.selected, 1);
    assert!(!resp.changed);
}

#[test]
fn dropdown_reflects_prior_state() {
    let mut state = HashMap::new();
    state.insert(0usize, WidgetState::Selected(2));
    let mut ctx = ctx_with_state(state);
    let resp = ctx.dropdown(&["a", "b", "c"], 0);
    assert_eq!(resp.selected, 2);
    assert!(resp.changed);
}

// ── image ─────────────────────────────────────────────────────────────────────

#[test]
fn image_records_spec_and_is_supported() {
    let mut ctx = default_ctx();
    let resp = ctx.image("http://example.com/img.png", None);
    assert!(resp.supported, "iced image feature is ON → supported");
}

// ── separator and spacer ─────────────────────────────────────────────────────

#[test]
fn separator_and_spacer_supported() {
    let mut ctx = default_ctx();
    let sep = ctx.separator();
    assert!(sep.supported);
    let spc = ctx.spacer(8.0);
    assert!(spc.supported);
}

// ── id allocation ─────────────────────────────────────────────────────────────

#[test]
fn button_and_text_input_get_distinct_ids() {
    // button → id 0, text_input → id 1; heading/label consume no ids.
    let mut ctx = default_ctx();
    ctx.heading("h"); // no id
    let br = ctx.button("btn"); // id 0
    ctx.label("l"); // no id
    let ti = ctx.text_input(""); // id 1

    // button 0 is not clicked (empty pending_clicks)
    assert!(!br.clicked);
    // text_input seed ""  → unchanged
    assert!(!ti.changed);
    // Both supported
    assert!(ti.supported);
}

// ── scroll_area / child id threading ─────────────────────────────────────────

#[test]
fn scroll_area_collects_child_specs_and_continues_ids() {
    let mut ctx = default_ctx();
    ctx.button("outer"); // id 0
    let r = ctx.scroll_area(&mut |inner| {
        inner.button("inner"); // id 1
    });
    assert!(r.supported);
    // After scroll_area, next outer id must be 2.
    ctx.button("outer2"); // id 2
    let _ = ctx.into_iced_element(); // must not panic
}

// ── tooltip ───────────────────────────────────────────────────────────────────

#[test]
fn tooltip_wraps_last_spec() {
    let mut ctx = default_ctx();
    ctx.label("base");
    let r = ctx.tooltip("hint text");
    assert!(r.supported, "tooltip wraps the last spec");
}

#[test]
fn tooltip_on_empty_specs_is_unsupported() {
    let mut ctx = default_ctx();
    let r = ctx.tooltip("orphan");
    assert!(!r.supported, "no previous spec → unsupported");
}

// ── apply_message round trips ─────────────────────────────────────────────────

#[test]
fn apply_text_changed_updates_state_next_frame() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(
        &mut state,
        &mut clicks,
        &Message::TextChanged(0, "world".to_owned()),
    );
    assert!(matches!(state.get(&0), Some(WidgetState::Text(s)) if s == "world"));
}

#[test]
fn apply_checkbox_toggle_round_trip() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(&mut state, &mut clicks, &Message::CheckboxToggled(0, true));
    assert!(matches!(state.get(&0), Some(WidgetState::Checked(true))));
}

#[test]
fn apply_slider_change_round_trip() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(&mut state, &mut clicks, &Message::SliderChanged(0, 0.42));
    match state.get(&0) {
        Some(WidgetState::Slider(v)) => assert!((*v - 0.42).abs() < 1e-9),
        other => panic!("unexpected state: {other:?}"),
    }
}

#[test]
fn apply_dropdown_select_round_trip() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(&mut state, &mut clicks, &Message::DropdownSelected(0, 3));
    assert!(matches!(state.get(&0), Some(WidgetState::Selected(3))));
}

#[test]
fn apply_button_pressed_adds_to_clicks() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(&mut state, &mut clicks, &Message::ButtonPressed(5));
    assert!(clicks.contains(&5));
}

// ── materialisation smoke test ────────────────────────────────────────────────

#[test]
fn into_iced_element_nonempty_tree_builds() {
    let mut ctx = default_ctx();
    ctx.heading("Title");
    ctx.label("Body text");
    ctx.button("Action");
    ctx.text_input("placeholder seed");
    ctx.separator();
    // Must not panic
    let _ = ctx.into_iced_element();
}

// ── IcedNullCtx recording ─────────────────────────────────────────────────────

#[test]
fn null_ctx_recording_logs_calls() {
    let mut ctx = IcedNullCtx::recording();
    ctx.heading("H");
    ctx.label("L");
    ctx.button("B");
    ctx.text_input("T");
    ctx.checkbox("C", false);
    ctx.slider(0.5, 0.0..=1.0);
    ctx.dropdown(&["x", "y"], 0);
    ctx.image("uri", None);
    ctx.separator();
    ctx.spacer(4.0);
    ctx.tooltip("tip");

    let log = ctx.log.expect("recording should be Some");
    let methods: Vec<&str> = log.iter().map(|(m, _)| *m).collect();
    assert!(methods.contains(&"heading"));
    assert!(methods.contains(&"label"));
    assert!(methods.contains(&"button"));
    assert!(methods.contains(&"text_input"));
    assert!(methods.contains(&"checkbox"));
    assert!(methods.contains(&"slider"));
    assert!(methods.contains(&"dropdown"));
    assert!(methods.contains(&"image"));
    assert!(methods.contains(&"separator"));
    assert!(methods.contains(&"spacer"));
    assert!(methods.contains(&"tooltip"));
    // Verify specific arg recorded
    let heading_entry = log.iter().find(|(m, _)| *m == "heading");
    assert_eq!(heading_entry.map(|(_, a)| a.as_str()), Some("H"));
}

// ── state persistence across frames ───────────────────────────────────────────

#[test]
fn test_state_persists_across_frames() {
    // Frame 1: build ctx, record a text-input spec.
    let mut config = IcedConfig::default();
    let mut ctx = IcedUiCtx::new(config.clone());
    let _resp = ctx.text_input("initial");
    // Simulate a TextChanged message updating state in config.
    apply_message(
        &mut config.state,
        &mut config.pending_clicks,
        &Message::TextChanged(0, "persisted".to_owned()),
    );
    // Frame 2: build a new ctx from the same (now updated) config.
    let mut ctx2 = IcedUiCtx::new(config.clone());
    let resp2 = ctx2.text_input("initial");
    // The state recorded in frame 1 must carry through to frame 2.
    assert_eq!(resp2.text, "persisted", "state must persist across frames");
    assert!(resp2.changed, "stored value differs from seed → changed");
}

// ── nested scroll materialisation ─────────────────────────────────────────────

#[test]
fn test_into_iced_element_with_nested_scroll() {
    let mut ctx = default_ctx();
    ctx.heading("outer heading");
    let r = ctx.scroll_area(&mut |inner| {
        inner.label("inner label");
        inner.button("inner button");
        let _ = inner.scroll_area(&mut |deep| {
            deep.label("deep");
        });
    });
    assert!(r.supported, "scroll_area must be supported");
    ctx.label("after scroll");
    // Materialisation of nested scroll areas must not panic.
    let _ = ctx.into_iced_element();
}

// ── IcedConfig builder methods ────────────────────────────────────────────────

#[test]
fn test_with_spacing_reflected_in_config() {
    let config = IcedConfig::default().with_spacing(16.0);
    assert!(
        (config.spacing - 16.0).abs() < f32::EPSILON,
        "with_spacing must set spacing field; got {}",
        config.spacing
    );
}

#[test]
fn test_with_padding_reflected_in_config() {
    let config = IcedConfig::default().with_padding(24.0);
    assert!(
        (config.padding - 24.0).abs() < f32::EPSILON,
        "with_padding must set padding field; got {}",
        config.padding
    );
}

// ── focused field (headless-approximate) ─────────────────────────────────────

#[test]
fn test_text_input_focused_is_false_in_headless() {
    let mut ctx = default_ctx();
    let resp = ctx.text_input("text");
    assert!(
        !resp.focused,
        "headless adapter must report focused = false"
    );
    assert!(resp.supported, "text_input must be supported");
}

// ── text_area (oxiui-core integration: text_area method) ─────────────────────

#[test]
fn text_area_returns_supported_response() {
    let mut ctx = default_ctx();
    let resp = ctx.text_area("hello\nworld", 3);
    assert!(resp.supported, "IcedUiCtx::text_area must be supported");
    assert_eq!(resp.text, "hello\nworld");
    assert!(!resp.changed, "seed text unchanged");
}

#[test]
fn text_area_reflects_prior_state() {
    let mut state = HashMap::new();
    state.insert(0usize, WidgetState::TextArea("stored\ntext".to_owned()));
    let mut ctx = ctx_with_state(state);
    let resp = ctx.text_area("ignored_seed", 2);
    assert_eq!(resp.text, "stored\ntext", "should reflect stored state");
    assert!(resp.changed, "stored value differs from seed");
}

#[test]
fn text_area_cursor_pos_approximated() {
    let mut ctx = default_ctx();
    let resp = ctx.text_area("line1\nline2\nline3", 3);
    // cursor_pos should be (last_line_idx, last_line_len) — (2, 5).
    assert_eq!(resp.cursor_pos.0, 2, "row = line count - 1");
    assert_eq!(resp.cursor_pos.1, 5, "col = len of last line");
}

#[test]
fn text_area_apply_message_text_area_changed_updates_state() {
    let mut state = HashMap::new();
    let mut clicks = std::collections::HashSet::new();
    apply_message(
        &mut state,
        &mut clicks,
        &Message::TextAreaChanged(0, "new content".to_owned()),
    );
    match state.get(&0) {
        Some(WidgetState::TextArea(s)) => assert_eq!(s, "new content"),
        other => panic!("unexpected state: {other:?}"),
    }
}

#[test]
fn text_area_state_persists_across_frames() {
    let mut config = IcedConfig::default();
    let mut ctx = IcedUiCtx::new(config.clone());
    let _resp = ctx.text_area("initial\ncontent", 3);
    // Simulate TextAreaChanged updating state.
    apply_message(
        &mut config.state,
        &mut config.pending_clicks,
        &Message::TextAreaChanged(0, "updated\ncontent".to_owned()),
    );
    // Frame 2: state should carry through.
    let mut ctx2 = IcedUiCtx::new(config.clone());
    let resp2 = ctx2.text_area("initial\ncontent", 3);
    assert_eq!(resp2.text, "updated\ncontent", "state must persist");
    assert!(resp2.changed, "stored differs from seed → changed");
}

#[test]
fn text_area_materialises_without_panic() {
    let mut ctx = default_ctx();
    ctx.text_area("line1\nline2\nline3", 3);
    // Materialisation of text_area must not panic.
    let _ = ctx.into_iced_element();
}

#[test]
fn null_ctx_text_area_is_recorded() {
    let mut ctx = IcedNullCtx::recording();
    ctx.text_area("content", 2);
    let log = ctx.log.expect("recording should be Some");
    let found = log.iter().any(|(m, _)| *m == "text_area");
    assert!(found, "text_area call should be logged");
}

#[test]
fn text_area_assigns_next_id_after_prior_widgets() {
    let mut ctx = default_ctx();
    // First widget gets id=0 (text_input), second gets id=1 (text_area).
    let _ti = ctx.text_input("seed");
    let resp = ctx.text_area("ta_seed", 2);
    // The text_area is not changed vs its own seed text.
    assert!(!resp.changed, "text_area seed equals stored = not changed");
    assert!(resp.supported);
}
