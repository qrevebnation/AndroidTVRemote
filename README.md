# Android TV Remote

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows%20%7C%20macOS-informational)](https://github.com/qrevebnation/AndroidTVRemote)
[![Theme](https://img.shields.io/badge/Theme-Catppuccin%20Mocha-cba6f7.svg)](https://github.com/catppuccin/catppuccin)

Desktop remote control untuk Google TV / Android TV berbasis Rust dan Slint dengan fitur live view layar TV langsung di dalam aplikasi.

---

## Fitur

- **Embedded Live Screen (360p @ 30 FPS)**: Stream layar TV real-time langsung di dalam window remote (tersedia dalam mode portrait dan fullscreen).
- **Protokol Asli TLS 1.3 (Port 6466)**: Komunikasi terenkripsi langsung via Google TV Remote v2 protocol.
- **Navigasi Keyboard**: Kontrol penuh tombol remote via keyboard fisik.
- **Session Privacy**: IP otomatis disembunyikan saat terhubung.
- **Catppuccin Mocha UI**: Antarmuka responsif dan hemat memori.

---

## Pintasan Keyboard

| Tombol | Aksi |
| :--- | :--- |
| **Arrow Keys** | DPAD Up / Down / Left / Right |
| **Enter / Space** | DPAD Center (OK) |
| **Esc / Backspace** | Back |
| **H / Home** | Home |
| **+ / -** | Volume Up / Down |
| **M** | Mute |
| **P** | Power |
| **K** | Play / Pause |

---

## Prasyarat

Memerlukan `adb` dan `ffmpeg` yang terpasang di sistem:

### Windows
```powershell
winget install Google.PlatformTools Gyan.FFmpeg
```

### macOS
```bash
brew install android-platform-tools ffmpeg
```

### Linux
```bash
# Ubuntu / Debian
sudo apt install -y android-tools-adb ffmpeg

# Arch Linux
sudo pacman -S android-tools ffmpeg
```

---

## Penggunaan

1. **Aktifkan Debugging di TV**:
   - Buka **Settings** $\rightarrow$ **System** $\rightarrow$ **About**.
   - Klik **Build** 7 kali untuk membuka Developer Options.
   - Buka **Settings** $\rightarrow$ **System** $\rightarrow$ **Developer Options** $\rightarrow$ aktifkan **Network / USB Debugging**.
2. **Koneksi**:
   - Buka aplikasi, masukkan IP TV, klik **Connect**.
   - Jika pertama kali, izinkan RSA prompt pada TV dan masukkan 6-digit PIN yang muncul.

---

## Build

```bash
git clone https://github.com/qrevebnation/AndroidTVRemote.git
cd AndroidTVRemote
cargo build --release
```

Output binary:
- **Windows**: `target/release/atv-remote.exe`
- **macOS / Linux**: `target/release/atv-remote`

---

## Lisensi

[MIT](LICENSE)
