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

pub fn get_adb_binary() -> &'static str {
    if Path::new("/home/mnrrochman/android-sdk/platform-tools/adb").exists() {
        "/home/mnrrochman/android-sdk/platform-tools/adb"
    } else {
        "adb"
    }
}

pub fn send_adb_key(tv_ip: &str, key_name: &str) {
    let android_code = match key_name {
        "DPAD_UP" => "19",
        "DPAD_DOWN" => "20",
        "DPAD_LEFT" => "21",
        "DPAD_RIGHT" => "22",
        "DPAD_CENTER" => "23",
        "BACK" => "4",
        "HOME" => "3",
        "VOLUME_UP" => "24",
        "VOLUME_DOWN" => "25",
        "MUTE" => "164",
        "POWER" => "26",
        "PLAY_PAUSE" => "85",
        "SETTINGS" => "176",
        "MENU" => "82",
        _ => return,
    };
    let ip = tv_ip.trim().to_string();
    if ip.is_empty() {
        return;
    }
    let adb_bin = get_adb_binary();
    std::thread::spawn(move || {
        let _ = SysCommand::new(adb_bin)
            .args(["-s", &format!("{}:5555", ip), "shell", "input", "keyevent", android_code])
            .spawn();
    });
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
        let adb_bin = get_adb_binary();

        // 1. Ensure ADB is connected
        let _ = SysCommand::new(adb_bin)
            .args(["connect", &format!("{}:5555", ip)])
            .output();

        // 2. Launch scrcpy with --no-audio (so TV sound stays on the TV speakers!)
        let _ = SysCommand::new("scrcpy")
            .args([
                "-s",
                &format!("{}:5555", ip),
                "--no-audio",
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
