use atv_remote::app::{init_state, launch_mini_screen, map_key_string};
use atv_remote::pairing::start_pairing;
use atv_remote::remote::{run_remote_session, Command as RemoteCommand, ConnectionState};
use atv_remote::ui::AppWindow;
use slint::ComponentHandle;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_window = AppWindow::new()?;
    let state = init_state()?;

    // Populate initial UI state from saved config
    {
        let state_guard = state.blocking_lock();
        if let Some(ip) = &state_guard.config.last_tv_ip {
            app_window.set_tv_ip(ip.clone().into());
        }
    }

    // Start background Tokio runtime on a dedicated thread
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    // Background worker thread
    let state_clone = Arc::clone(&state);
    let weak_window = app_window.as_weak();
    std::thread::spawn(move || {
        rt.block_on(async move {
            let mut current_session: Option<tokio::task::JoinHandle<()>> = None;

            while let Some(action) = action_rx.recv().await {
                match action {
                    Action::Connect(ip) => {
                        if let Some(handle) = current_session.take() {
                            handle.abort();
                        }

                        let weak = weak_window.clone();
                        let state = Arc::clone(&state_clone);

                        current_session = Some(tokio::spawn(async move {
                            // Update UI status to Connecting
                            let _ = weak.upgrade_in_event_loop(|w| {
                                w.set_connection_status("Connecting...".into());
                                w.set_status_color(slint::Color::from_rgb_u8(255, 179, 0));
                                // Amber
                            });

                            let (cert, key, is_paired) = {
                                let g = state.lock().await;
                                (g.cert.clone(), g.key.clone_key(), g.config.paired)
                            };

                            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                            let (status_tx, mut status_rx) = mpsc::unbounded_channel();

                            // Attempt direct remote session if previously paired
                            if is_paired {
                                {
                                    let mut g = state.lock().await;
                                    g.cmd_tx = Some(cmd_tx.clone());
                                }

                                let weak_stat = weak.clone();
                                tokio::spawn(async move {
                                    while let Some(stat) = status_rx.recv().await {
                                        let _ =
                                            weak_stat.upgrade_in_event_loop(move |w| match stat {
                                                ConnectionState::Connecting => {
                                                    w.set_connection_status("Connecting...".into());
                                                    w.set_status_color(slint::Color::from_rgb_u8(
                                                        255, 179, 0,
                                                    ));
                                                }
                                                ConnectionState::Connected => {
                                                    w.set_connection_status("Connected".into());
                                                    w.set_status_color(slint::Color::from_rgb_u8(
                                                        76, 175, 80,
                                                    )); // Green
                                                }
                                                ConnectionState::Disconnected(reason) => {
                                                    w.set_connection_status(
                                                        format!("Disconnected: {reason}").into(),
                                                    );
                                                    w.set_status_color(slint::Color::from_rgb_u8(
                                                        244, 67, 54,
                                                    )); // Red
                                                }
                                                ConnectionState::ActiveApp(app) => {
                                                    w.set_active_app(app.into());
                                                }
                                            });
                                    }
                                });

                                if let Err(e) = run_remote_session(
                                    &ip,
                                    cert.clone(),
                                    key.clone_key(),
                                    cmd_rx,
                                    status_tx,
                                )
                                .await
                                {
                                    eprintln!("Session error: {e}");
                                }
                            } else {
                                // Initiate Pairing flow
                                let (pin_tx, pin_rx) = oneshot::channel();
                                {
                                    let mut g = state.lock().await;
                                    g.pin_tx = Some(pin_tx);
                                }

                                let _ = weak.upgrade_in_event_loop(|w| {
                                    w.set_connection_status("Enter PIN on screen".into());
                                    w.set_status_color(slint::Color::from_rgb_u8(33, 150, 243)); // Blue
                                    w.set_show_pairing_modal(true);
                                });

                                match start_pairing(&ip, &cert, &key, pin_rx).await {
                                    Ok(()) => {
                                        let _ = weak.upgrade_in_event_loop(|w| {
                                            w.set_show_pairing_modal(false);
                                            w.set_connection_status("Paired! Connecting...".into());
                                        });

                                        // Save paired status
                                        {
                                            let mut g = state.lock().await;
                                            g.config.paired = true;
                                            g.config.last_tv_ip = Some(ip.clone());
                                            let _ = g.config.save();
                                        }

                                        // Connect to remote service after pairing
                                        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                                        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
                                        {
                                            let mut g = state.lock().await;
                                            g.cmd_tx = Some(cmd_tx);
                                        }

                                        let weak_stat = weak.clone();
                                        tokio::spawn(async move {
                                            while let Some(stat) = status_rx.recv().await {
                                                let _ = weak_stat.upgrade_in_event_loop(move |w| {
                                                    match stat {
                                                        ConnectionState::Connected => {
                                                            w.set_connection_status(
                                                                "Connected".into(),
                                                            );
                                                            w.set_status_color(
                                                                slint::Color::from_rgb_u8(
                                                                    76, 175, 80,
                                                                ),
                                                            );
                                                        }
                                                        ConnectionState::Disconnected(reason) => {
                                                            w.set_connection_status(
                                                                format!("Disconnected: {reason}")
                                                                    .into(),
                                                            );
                                                            w.set_status_color(
                                                                slint::Color::from_rgb_u8(
                                                                    244, 67, 54,
                                                                ),
                                                            );
                                                        }
                                                        _ => {}
                                                    }
                                                });
                                            }
                                        });

                                        let _ =
                                            run_remote_session(&ip, cert, key, cmd_rx, status_tx)
                                                .await;
                                    }
                                    Err(e) => {
                                        let _ = weak.upgrade_in_event_loop(move |w| {
                                            w.set_show_pairing_modal(false);
                                            w.set_connection_status(
                                                format!("Pairing failed: {e}").into(),
                                            );
                                            w.set_status_color(slint::Color::from_rgb_u8(
                                                244, 67, 54,
                                            ));
                                        });
                                    }
                                }
                            }
                        }));
                    }

                    Action::SendKey(key_name) => {
                        if let Some((code, dir)) = map_key_string(&key_name) {
                            let g = state_clone.lock().await;
                            if let Some(tx) = &g.cmd_tx {
                                let _ = tx.send(RemoteCommand::SendKey(code, dir));
                            }
                        }
                    }

                    Action::SubmitPin(pin) => {
                        let mut g = state_clone.lock().await;
                        if let Some(tx) = g.pin_tx.take() {
                            let _ = tx.send(pin.trim().to_uppercase());
                        }
                    }

                    Action::CancelPairing => {
                        let mut g = state_clone.lock().await;
                        g.pin_tx = None;
                    }

                    Action::LaunchMiniScreen(ip) => {
                        launch_mini_screen(&ip);
                    }
                }
            }
        });
    });

    // Wire Slint callbacks
    let act_tx = action_tx.clone();
    app_window.on_send_key(move |key| {
        let _ = act_tx.send(Action::SendKey(key.to_string()));
    });

    let act_tx = action_tx.clone();
    app_window.on_connect_tv(move |ip| {
        let _ = act_tx.send(Action::Connect(ip.to_string()));
    });

    let act_tx = action_tx.clone();
    app_window.on_submit_pin(move |pin| {
        let _ = act_tx.send(Action::SubmitPin(pin.to_string()));
    });

    let act_tx = action_tx.clone();
    app_window.on_cancel_pairing(move || {
        let _ = act_tx.send(Action::CancelPairing);
    });

    let act_tx = action_tx;
    let weak_ip = app_window.as_weak();
    app_window.on_launch_mini_screen(move || {
        let ip = weak_ip
            .upgrade()
            .map(|w| w.get_tv_ip().to_string())
            .unwrap_or_default();
        let _ = act_tx.send(Action::LaunchMiniScreen(ip));
    });

    app_window.run()?;
    Ok(())
}

enum Action {
    Connect(String),
    SendKey(String),
    SubmitPin(String),
    CancelPairing,
    LaunchMiniScreen(String),
}
