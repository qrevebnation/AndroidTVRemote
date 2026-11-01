use atv_remote::ui::AppWindow;

#[test]
fn test_app_window_instantiation_and_properties() {
    let app = AppWindow::new().expect("Failed to create AppWindow");
    assert!(!app.get_show_pairing_modal());

    app.set_connection_status("Connected".into());
    assert_eq!(app.get_connection_status(), "Connected");

    app.set_show_pairing_modal(true);
    assert!(app.get_show_pairing_modal());
}
