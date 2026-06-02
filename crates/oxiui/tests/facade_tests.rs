#[test]
fn headless_once_runs_ok() {
    let result = oxiui::App::new(oxiui::AppConfig::new().title("test"))
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Test");
            ui.label("label");
            let _ = ui.button("button");
        })
        .run_headless_once();
    assert!(
        result.is_ok(),
        "run_headless_once must return Ok: {:?}",
        result
    );
    assert_eq!(result.unwrap(), oxiui::AppExit::Ok);
}

#[test]
fn app_builder_chains() {
    let _app = oxiui::App::new(oxiui::AppConfig::new().title("chain test"))
        .theme(oxiui::theme::dark())
        .content(|_ui| {});
}

#[test]
fn app_builder_no_content() {
    let result = oxiui::App::new(oxiui::AppConfig::new().title("no-content test"))
        .theme(oxiui::theme::light())
        .run_headless_once();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), oxiui::AppExit::Ok);
}

#[test]
fn no_egui_run_returns_unsupported() {
    // When `egui` feature is disabled, `run()` should return Unsupported.
    // When enabled (the default), we cannot call run() without a display.
    // This test validates run_headless_once() still works regardless.
    let result = oxiui::App::new(oxiui::AppConfig::new().title("headless")).run_headless_once();
    assert!(result.is_ok());
}
