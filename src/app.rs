use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::Read;
use std::path::Path;
use std::process::{Command as SysCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use slint::Weak;

use crate::cert::load_or_generate_identity;
use crate::config::AppConfig;
use crate::proto::remote::{RemoteDirection, RemoteKeyCode};
use crate::remote::Command as RemoteCommand;
use crate::ui::AppWindow;

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

pub fn capture_screen_snapshot_indexed(tv_ip: &str, index: u8) -> Option<std::path::PathBuf> {
    let clean_ip = tv_ip.trim();
    if clean_ip.is_empty() {
        return None;
    }
    let adb_bin = get_adb_binary();
    let target = format!("{}:5555", clean_ip);
    let tmp_path = std::env::temp_dir().join(format!("atv_remote_screen_{}.png", index));
    let remote_path = format!("/sdcard/atv_screen_{}.png", index);

    let _ = SysCommand::new(adb_bin)
        .args(["-s", &target, "shell", "screencap", &remote_path])
        .output();

    let pull_res = SysCommand::new(adb_bin)
        .args(["-s", &target, "pull", &remote_path, tmp_path.to_str()?])
        .output();

    if matches!(pull_res, Ok(out) if out.status.success()) {
        Some(tmp_path)
    } else {
        None
    }
}

pub fn capture_screen_snapshot(tv_ip: &str) -> Option<std::path::PathBuf> {
    capture_screen_snapshot_indexed(tv_ip, 0)
}

pub fn start_inapp_stream(
    tv_ip: &str,
    weak_window: Weak<AppWindow>,
    is_running: Arc<AtomicBool>,
) {
    let clean_ip = tv_ip.trim().trim_end_matches(":5555");
    if clean_ip.is_empty() {
        return;
    }
    let ip = clean_ip.to_string();
    let adb_bin = get_adb_binary();

    std::thread::spawn(move || {
        let target = format!("{}:5555", ip);
        println!("[Stream] Connecting to {target}...");

        // Ensure ADB is connected and kill any stale screenrecord process on the TV
        let _ = SysCommand::new(adb_bin)
            .args(["connect", &target])
            .output();
        let _ = SysCommand::new(adb_bin)
            .args(["-s", &target, "shell", "pkill", "-f", "screenrecord"])
            .output();

        // 1. Spawn adb exec-out screenrecord (360p H.264 stream to stdout)
        let mut adb_child = match SysCommand::new(adb_bin)
            .args([
                "-s",
                &target,
                "exec-out",
                "screenrecord",
                "--output-format=h264",
                "--size",
                "640x360",
                "--bit-rate",
                "800000",
                "--time-limit",
                "0",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Stream] Failed to spawn adb screenrecord: {e}");
                return;
            }
        };

        let adb_stdout = match adb_child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = adb_child.kill();
                return;
            }
        };

        // 2. Spawn ffmpeg to decode H.264 directly into raw rgb24 frames
        let mut ffmpeg_child = match SysCommand::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-probesize",
                "32768",
                "-analyzeduration",
                "500000",
                "-f",
                "h264",
                "-i",
                "-",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-s",
                "640x360",
                "-",
            ])
            .stdin(adb_stdout)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Stream] Failed to spawn ffmpeg: {e}");
                let _ = adb_child.kill();
                return;
            }
        };

        let mut ffmpeg_stdout = match ffmpeg_child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = ffmpeg_child.kill();
                let _ = adb_child.kill();
                return;
            }
        };

        const WIDTH: u32 = 640;
        const HEIGHT: u32 = 360;
        const FRAME_SIZE: usize = (WIDTH * HEIGHT * 3) as usize;
        let mut buffer = vec![0u8; FRAME_SIZE];
        let mut frame_count = 0u64;

        while is_running.load(Ordering::SeqCst) {
            if let Err(e) = ffmpeg_stdout.read_exact(&mut buffer) {
                eprintln!("[Stream] Read error: {e}");
                break;
            }

            frame_count += 1;
            let frame_bytes = buffer.clone();
            let weak = weak_window.clone();
            let _ = weak.upgrade_in_event_loop(move |w| {
                let pixel_buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::clone_from_slice(
                    &frame_bytes,
                    WIDTH,
                    HEIGHT,
                );
                w.set_tv_screen_image(slint::Image::from_rgb8(pixel_buffer));
                w.set_has_screen_image(true);
                w.set_is_streaming(true);
            });
        }

        println!("[Stream] Stream stopped after {frame_count} frames");
        let _ = ffmpeg_child.kill();
        let _ = adb_child.kill();
        let _ = SysCommand::new(adb_bin)
            .args(["-s", &target, "shell", "pkill", "-f", "screenrecord"])
            .output();

        let _ = weak_window.upgrade_in_event_loop(|w| {
            w.set_is_streaming(false);
            w.set_has_screen_image(false);
        });
    });
}
