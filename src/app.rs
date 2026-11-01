use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::path::Path;
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
    let clean_ip = tv_ip.trim();
    if clean_ip.is_empty() {
        return;
    }
    let ip = clean_ip.to_string();
    std::thread::spawn(move || {
        let adb_bin = if Path::new("/home/mnrrochman/android-sdk/platform-tools/adb").exists() {
            "/home/mnrrochman/android-sdk/platform-tools/adb"
        } else {
            "adb"
        };
        // Connect ADB over Wi-Fi
        let _ = SysCommand::new(adb_bin)
            .args(["connect", &format!("{}:5555", ip)])
            .output();

        // Launch scrcpy over Wi-Fi with compact floating window
        let _ = SysCommand::new("scrcpy")
            .args([
                "-s",
                &format!("{}:5555", ip),
                "--always-on-top",
                "--window-title",
                "Google TV Mini Screen",
                "--window-width",
                "480",
                "--max-fps",
                "30",
            ])
            .spawn();
    });
}
