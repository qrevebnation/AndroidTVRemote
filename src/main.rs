use atv_remote::app::{
    get_adb_binary, init_state, map_key_string, send_adb_key, start_inapp_stream, SharedState,
};
use atv_remote::pairing::start_pairing;
use atv_remote::remote::{run_remote_session, Command as RemoteCommand, ConnectionState};
use atv_remote::ui::AppWindow;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use slint::{ComponentHandle, Weak};
use std::process::Command as SysCommand;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let stream_running = Arc::new(AtomicBool::new(false));

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
                        let stream_running_conn = Arc::clone(&stream_running);

                        // Warm up ADB in background
                        let ip_clone = ip.clone();
                        std::thread::spawn(move || {
                            let adb_bin = get_adb_binary();
                            let _ = SysCommand::new(adb_bin)
                                .args(["connect", &format!("{}:5555", ip_clone.trim())])
                                .output();
                        });

                        current_session = Some(tokio::spawn(async move {
                            // Update UI status to Connecting
                            let _ = weak.upgrade_in_event_loop(|w| {
                                w.set_connection_status("Connecting...".into());
                                w.set_status_color(slint::Color::from_rgb_u8(255, 179, 0)); // Amber
                            });

                            let (cert, key, is_paired) = {
                                let g = state.lock().await;
                                (g.cert.clone(), g.key.clone_key(), g.config.paired)
                            };

                            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                            let (status_tx, mut status_rx) = mpsc::unbounded_channel();

                            if is_paired {
                                {
                                    let mut g = state.lock().await;
                                    g.cmd_tx = Some(cmd_tx.clone());
                                }

                                let weak_stat = weak.clone();
                                let stream_run = Arc::clone(&stream_running_conn);
                                let ip_stream = ip.clone();
                                tokio::spawn(async move {
                                    while let Some(stat) = status_rx.recv().await {
                                        let stream_run_sub = Arc::clone(&stream_run);
                                        let ip_sub = ip_stream.clone();
                                        let weak_sub = weak_stat.clone();
                                        let _ = weak_stat.upgrade_in_event_loop(move |w| match stat {
                                            ConnectionState::Connecting => {
                                                w.set_connection_status("Connecting...".into());
                                                w.set_status_color(slint::Color::from_rgb_u8(255, 179, 0));
                                                w.set_is_connected(false);
                                            }
                                            ConnectionState::Connected => {
                                                w.set_connection_status("Connected".into());
                                                w.set_status_color(slint::Color::from_rgb_u8(76, 175, 80)); // Green
                                                w.set_is_connected(true);
                                                w.set_is_streaming(true);
                                                if !stream_run_sub.load(Ordering::SeqCst) {
                                                    stream_run_sub.store(true, Ordering::SeqCst);
                                                    start_inapp_stream(&ip_sub, weak_sub, stream_run_sub);
                                                }
                                            }
                                            ConnectionState::Disconnected(reason) => {
                                                w.set_connection_status(format!("Disconnected: {reason}").into());
                                                w.set_status_color(slint::Color::from_rgb_u8(244, 67, 54)); // Red
                                                w.set_is_connected(false);
                                                w.set_has_screen_image(false);
                                                w.set_is_streaming(false);
                                            }
                                            ConnectionState::ActiveApp(app) => {
                                                w.set_active_app(app.into());
                                            }
                                        });
                                    }
                                });

                                let session_res = run_remote_session(
                                    &ip,
                                    cert.clone(),
                                    key.clone_key(),
                                    cmd_rx,
                                    status_tx,
                                )
                                .await;

                                if let Err(e) = session_res {
                                    eprintln!("Session error on 6466: {e}");
                                    let err_str = format!("{e}");
                                    // If TV unpairs or rejects certificate, automatically fallback to pairing flow
                                    if err_str.contains("CertificateUnknown") || err_str.contains("certificate unknown") {
                                        eprintln!("Certificate not recognized by TV. Initiating pairing...");
                                        {
                                            let mut g = state.lock().await;
                                            g.config.paired = false;
                                            let _ = g.config.save();
                                        }
                                        execute_pairing(&ip, cert, key, weak, state, stream_running_conn).await;
                                    }
                                }
                            } else {
                                execute_pairing(&ip, cert, key, weak, state, stream_running_conn).await;
                            }
                        }));
                    }

                    Action::SendKey(key_name) => {
                        let g = state_clone.lock().await;
                        let last_ip = g.config.last_tv_ip.clone().unwrap_or_else(|| "192.168.1.12".to_string());
                        let mut sent_tls = false;

                        if let Some((code, dir)) = map_key_string(&key_name) {
                            if let Some(tx) = &g.cmd_tx {
                                if tx.send(RemoteCommand::SendKey(code, dir)).is_ok() {
                                    sent_tls = true;
                                }
                            }
                        }

                        // Only fallback to ADB if TLS remote session is not connected
                        if !sent_tls {
                            send_adb_key(&last_ip, &key_name);
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

                    Action::Disconnect => {
                        stream_running.store(false, Ordering::SeqCst);
                        let _ = SysCommand::new("pkill")
                            .args(["-f", "screenrecord"])
                            .output();
                        let _ = SysCommand::new("pkill")
                            .args(["-f", "scrcpy"])
                            .output();
                        if let Some(handle) = current_session.take() {
                            handle.abort();
                        }
                        {
                            let mut g = state_clone.lock().await;
                            g.cmd_tx = None;
                        }
                        let _ = weak_window.upgrade_in_event_loop(|w| {
                            w.set_connection_status("Disconnected".into());
                            w.set_status_color(slint::Color::from_rgb_u8(243, 139, 168)); // Red
                            w.set_active_app("".into());
                            w.set_is_connected(false);
                            w.set_has_screen_image(false);
                            w.set_is_streaming(false);
                        });
                    }

                    Action::ToggleMiniScreen => {
                        let is_active = stream_running.load(Ordering::SeqCst);
                        if is_active {
                            stream_running.store(false, Ordering::SeqCst);
                            let _ = SysCommand::new("pkill")
                                .args(["-f", "screenrecord"])
                                .output();
                            let _ = weak_window.upgrade_in_event_loop(|w| {
                                w.set_is_streaming(false);
                                w.set_has_screen_image(false);
                            });
                        } else {
                            let last_ip = weak_window
                                .upgrade()
                                .map(|w| w.get_tv_ip().to_string())
                                .filter(|s| !s.trim().is_empty())
                                .unwrap_or_else(|| "192.168.1.12".to_string());
                            stream_running.store(true, Ordering::SeqCst);
                            let _ = weak_window.upgrade_in_event_loop(|w| {
                                w.set_show_mini_screen_portrait(true);
                                w.set_is_streaming(true);
                            });
                            start_inapp_stream(&last_ip, weak_window.clone(), stream_running.clone());
                        }
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
    app_window.on_disconnect_tv(move || {
        let _ = act_tx.send(Action::Disconnect);
    });

    let act_tx = action_tx.clone();
    app_window.on_submit_pin(move |pin| {
        let _ = act_tx.send(Action::SubmitPin(pin.to_string()));
    });

    let act_tx = action_tx.clone();
    app_window.on_cancel_pairing(move || {
        let _ = act_tx.send(Action::CancelPairing);
    });

    let act_tx = action_tx.clone();
    app_window.on_toggle_mini_screen(move || {
        let _ = act_tx.send(Action::ToggleMiniScreen);
    });

    app_window.run()?;
    Ok(())
}

async fn execute_pairing(
    ip: &str,
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    weak: Weak<AppWindow>,
    state: SharedState,
    stream_running: Arc<AtomicBool>,
) {
    let (pin_tx, pin_rx) = oneshot::channel();
    {
        let mut g = state.lock().await;
        g.pin_tx = Some(pin_tx);
    }

    let _ = weak.upgrade_in_event_loop(|w| {
        w.set_connection_status("Connecting for pairing...".into());
        w.set_status_color(slint::Color::from_rgb_u8(33, 150, 243)); // Blue
        w.set_show_pairing_modal(true);
    });

    match start_pairing(ip, &cert, &key, pin_rx).await {
        Ok(()) => {
            let _ = weak.upgrade_in_event_loop(|w| {
                w.set_show_pairing_modal(false);
                w.set_connection_status("Paired! Connecting...".into());
            });

            // Save paired status
            {
                let mut g = state.lock().await;
                g.config.paired = true;
                g.config.last_tv_ip = Some(ip.to_string());
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
            let stream_run = Arc::clone(&stream_running);
            let ip_str = ip.to_string();
            tokio::spawn(async move {
                while let Some(stat) = status_rx.recv().await {
                    let stream_sub = Arc::clone(&stream_run);
                    let ip_sub = ip_str.clone();
                    let weak_sub = weak_stat.clone();
                    let _ = weak_stat.upgrade_in_event_loop(move |w| match stat {
                        ConnectionState::Connected => {
                            w.set_connection_status("Connected".into());
                            w.set_status_color(slint::Color::from_rgb_u8(76, 175, 80));
                            w.set_is_connected(true);
                            w.set_is_streaming(true);
                            if !stream_sub.load(Ordering::SeqCst) {
                                stream_sub.store(true, Ordering::SeqCst);
                                start_inapp_stream(&ip_sub, weak_sub, stream_sub);
                            }
                        }
                        ConnectionState::Disconnected(reason) => {
                            w.set_connection_status(format!("Disconnected: {reason}").into());
                            w.set_status_color(slint::Color::from_rgb_u8(244, 67, 54));
                            w.set_is_connected(false);
                            w.set_has_screen_image(false);
                            w.set_is_streaming(false);
                        }
                        _ => {}
                    });
                }
            });

            let _ = run_remote_session(ip, cert, key, cmd_rx, status_tx).await;
        }
        Err(e) => {
            let _ = weak.upgrade_in_event_loop(move |w| {
                w.set_show_pairing_modal(false);
                w.set_connection_status(format!("Pairing failed: {e}").into());
                w.set_status_color(slint::Color::from_rgb_u8(244, 67, 54));
                w.set_is_connected(false);
            });
        }
    }
}

enum Action {
    Connect(String),
    Disconnect,
    SendKey(String),
    SubmitPin(String),
    CancelPairing,
    ToggleMiniScreen,
}
