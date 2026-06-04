//! Tests for the AccessKit a11y bridge (requires `a11y` feature).
//!
//! Run with: cargo nextest run -p oxiui-iced --features a11y

#![cfg(feature = "a11y")]

use oxiui_iced::{
    a11y_bridge::{spec_to_a11y_tree, IcedA11yConfig},
    adapter::{IcedSpan, WidgetSpec},
};
use std::borrow::Cow;

#[test]
fn empty_specs_produce_root_window_node() {
    let specs: Vec<WidgetSpec> = vec![];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(
        update.nodes.len(),
        1,
        "root window node only for empty spec list"
    );
    let (_, ref root) = update.nodes[0];
    assert_eq!(root.role(), accesskit::Role::Window);
}

#[test]
fn label_spec_maps_to_label_role_with_name() {
    let specs = vec![WidgetSpec::Label(Cow::Borrowed("Welcome"))];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::Label);
    assert_eq!(node.label(), Some("Welcome"));
}

#[test]
fn heading_maps_to_label_role() {
    let specs = vec![WidgetSpec::Heading(Cow::Borrowed("Section"))];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::Label);
    assert_eq!(node.label(), Some("Section"));
}

#[test]
fn button_maps_to_button_role() {
    let specs = vec![WidgetSpec::Button {
        id: 0,
        label: Cow::Borrowed("Submit"),
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::Button);
    assert_eq!(node.label(), Some("Submit"));
}

#[test]
fn text_input_maps_to_text_input_role() {
    let specs = vec![WidgetSpec::TextInput {
        id: 0,
        value: Cow::Borrowed("hello"),
        placeholder: Cow::Borrowed("Enter text"),
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::TextInput);
}

#[test]
fn text_area_maps_to_text_input_role() {
    let specs = vec![WidgetSpec::TextArea {
        id: 0,
        value: Cow::Borrowed("multi\nline"),
        min_rows: 3,
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::TextInput);
}

#[test]
fn checkbox_maps_to_checkbox_role() {
    let specs = vec![WidgetSpec::Checkbox {
        id: 0,
        label: Cow::Borrowed("Accept"),
        checked: true,
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::CheckBox);
}

#[test]
fn slider_maps_to_slider_role() {
    let specs = vec![WidgetSpec::Slider {
        id: 0,
        value: 42.0,
        start: 0.0,
        end: 100.0,
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::Slider);
    assert_eq!(node.numeric_value(), Some(42.0));
}

#[test]
fn separator_and_spacer_are_omitted() {
    let specs = vec![WidgetSpec::Separator, WidgetSpec::Spacer { size: 16.0 }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 1, "decorative widgets must be omitted");
}

#[test]
fn scroll_area_maps_to_scroll_view_with_children() {
    let specs = vec![WidgetSpec::Scroll {
        children: vec![
            WidgetSpec::Label(Cow::Borrowed("A")),
            WidgetSpec::Label(Cow::Borrowed("B")),
        ],
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    // Root + ScrollView + 2 labels = 4.
    assert_eq!(update.nodes.len(), 4);
    let (_, ref scroll_node) = update.nodes[1];
    assert_eq!(scroll_node.role(), accesskit::Role::ScrollView);
}

#[test]
fn modal_maps_to_dialog_with_title() {
    let specs = vec![WidgetSpec::Modal {
        title: Cow::Borrowed("Delete?"),
        children: vec![WidgetSpec::Button {
            id: 0,
            label: Cow::Borrowed("Yes"),
        }],
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    // Root + Dialog + Button = 3.
    assert_eq!(update.nodes.len(), 3);
    let (_, ref dialog) = update.nodes[1];
    assert_eq!(dialog.role(), accesskit::Role::Dialog);
    assert_eq!(dialog.label(), Some("Delete?"));
}

#[test]
fn rich_text_collapses_to_single_label() {
    let specs = vec![WidgetSpec::RichText(vec![
        IcedSpan {
            text: "Hello ".to_string(),
            color: None,
            bold: false,
            size: None,
        },
        IcedSpan {
            text: "World".to_string(),
            color: Some([255, 0, 0, 255]),
            bold: true,
            size: None,
        },
    ])];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.label(), Some("Hello World"));
}

#[test]
fn root_label_config_is_reflected() {
    let config = IcedA11yConfig::default().with_root_label("My Window");
    let update = spec_to_a11y_tree(&[], &config);
    let (_, ref root) = update.nodes[0];
    assert_eq!(root.label(), Some("My Window"));
}

#[test]
fn id_start_config_adjusts_root_id() {
    let config = IcedA11yConfig::default().with_id_start(500);
    let update = spec_to_a11y_tree(&[], &config);
    let (root_id, _) = &update.nodes[0];
    assert_eq!(root_id.0, 500);
}

#[test]
fn horizontal_layout_maps_to_group() {
    let specs = vec![WidgetSpec::Horizontal(vec![
        WidgetSpec::Label(Cow::Borrowed("a")),
        WidgetSpec::Button {
            id: 0,
            label: Cow::Borrowed("b"),
        },
    ])];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    // Root + Group + Label + Button = 4.
    assert_eq!(update.nodes.len(), 4);
    let (_, ref group) = update.nodes[1];
    assert_eq!(group.role(), accesskit::Role::Group);
}

#[test]
fn popup_maps_to_dialog_role() {
    let specs = vec![WidgetSpec::Popup {
        children: vec![WidgetSpec::Label(Cow::Borrowed("content"))],
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    // Root + Dialog + Label = 3.
    assert_eq!(update.nodes.len(), 3);
    let (_, ref dialog) = update.nodes[1];
    assert_eq!(dialog.role(), accesskit::Role::Dialog);
}

#[test]
fn image_carries_uri_as_name() {
    let specs = vec![WidgetSpec::Image {
        uri: Cow::Borrowed("banner.png"),
        size: None,
    }];
    let update = spec_to_a11y_tree(&specs, &IcedA11yConfig::default());
    assert_eq!(update.nodes.len(), 2);
    let (_, ref node) = update.nodes[1];
    assert_eq!(node.role(), accesskit::Role::Image);
    assert_eq!(node.label(), Some("banner.png"));
}
