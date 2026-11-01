use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::process::Command as SysCommand;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::cert::load_or_generate_identity;
use crate::config::AppConfig;
use crate::proto::remote::{RemoteDirection, RemoteKeyCode};
use crate::remote::Command as RemoteCommand;

pub fn map_key_string(key: &str) -> Option<(RemoteKeyCode, RemoteDirection)> {
    let code = match key {
        "DPAD_UP" => RemoteKeyCode::KeycodeDpadUp,
        "DPAD_DOWN" => RemoteKeyCode::KeycodeDpadDown,
        "DPAD_LEFT" => RemoteKeyCode::KeycodeDpadLeft,
        "DPAD_RIGHT" => RemoteKeyCode::KeycodeDpadRight,
        "DPAD_CENTER" => RemoteKeyCode::KeycodeDpadCenter,
        "BACK" => RemoteKeyCode::KeycodeBack,
        "HOME" => RemoteKeyCode::KeycodeHome,
        "VOLUME_UP" => RemoteKeyCode::KeycodeVolumeUp,
        "VOLUME_DOWN" => RemoteKeyCode::KeycodeVolumeDown,
        "MUTE" => RemoteKeyCode::KeycodeVolumeMute,
        "POWER" => RemoteKeyCode::KeycodePower,
        "PLAY_PAUSE" => RemoteKeyCode::KeycodeMediaPlayPause,
        "SETTINGS" => RemoteKeyCode::KeycodeSettings,
        "MENU" => RemoteKeyCode::KeycodeMenu,
        _ => return None,
    };
    Some((code, RemoteDirection::Short))
}

pub struct AppState {
    pub config: AppConfig,
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
    pub cmd_tx: Option<mpsc::UnboundedSender<RemoteCommand>>,
    pub pin_tx: Option<oneshot::Sender<String>>,
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn init_state() -> Result<SharedState, Box<dyn std::error::Error>> {
    let config = AppConfig::load();
    let (cert, key) = load_or_generate_identity()?;
    Ok(Arc::new(Mutex::new(AppState {
        config,
        cert,
        key,
        cmd_tx: None,
        pin_tx: None,
    })))
}

pub fn launch_mini_screen(tv_ip: &str) {
    if tv_ip.trim().is_empty() {
        return;
    }
    // Launch scrcpy over Wi-Fi with compact floating window
    let _ = SysCommand::new("scrcpy")
        .args([
            "--tcpip",
            tv_ip.trim(),
            "--always-on-top",
            "--window-title",
            "Google TV Mini Screen",
            "--window-width",
            "480",
            "--max-fps",
            "30",
        ])
        .spawn();
}
