//! Hello world OxiUI example — M1 gate.
fn main() -> Result<(), oxiui::UiError> {
    oxiui::App::new(oxiui::AppConfig::new().title("Hello OxiUI"))
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Hello, world!");
            ui.label("Pure-Rust UI — no GTK, no Qt, no SDL.");
            let _ = ui.button("Click me");
        })
        .run()?;
    Ok(())
}
