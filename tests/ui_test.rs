use atv_remote::ui::AppWindow;

#[test]
fn test_app_window_instantiation_and_properties() {
    let app = AppWindow::new().expect("Failed to create AppWindow");
    assert_eq!(app.get_connection_status(), "Disconnected");
    assert_eq!(app.get_show_pairing_modal(), false);

    app.set_connection_status("Connected".into());
    assert_eq!(app.get_connection_status(), "Connected");

    app.set_show_pairing_modal(true);
    assert_eq!(app.get_show_pairing_modal(), true);
}
