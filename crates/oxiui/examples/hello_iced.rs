//! Hello World via the iced backend.
//!
//! Demonstrates the iced UiCtx adapter with COOLJAPAN theming.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_iced --features iced -p oxiui
//! ```
//!
//! # Architecture note
//!
//! This example uses iced directly (not through the `App` facade) because
//! iced is retained-mode while the facade's content-closure API is
//! immediate-mode. M3 will wire the full closure → message round-trip through
//! the facade. At M2 this example is the canonical iced demo.
use iced::widget::{button, column, text};
use iced::{Element, Task};
use oxiui_iced::palette_to_iced_theme;

fn main() -> iced::Result {
    iced::application(|| (), update, view)
        .title("Hello OxiUI (iced)")
        .theme(theme)
        .run()
}

/// Build the COOLJAPAN theme for iced.
fn theme(_state: &()) -> iced::Theme {
    let theme_box = oxiui_theme::cooljapan_default();
    // We need to clone the palette out so the lifetime is not tied to theme_box
    let palette = theme_box.palette().clone();
    palette_to_iced_theme(&palette)
}

/// Messages for the hello_iced example.
#[derive(Debug, Clone)]
enum Message {
    /// User pressed the Quit button.
    Quit,
}

fn update(_state: &mut (), message: Message) -> Task<Message> {
    match message {
        Message::Quit => iced::exit(),
    }
}

fn view(_state: &()) -> Element<'_, Message> {
    column![
        text("Hello, world!").size(24),
        text("Pure-Rust UI — no GTK, no Qt, no SDL.").size(14),
        button("Quit").on_press(Message::Quit),
    ]
    .spacing(12)
    .padding(20)
    .into()
}
