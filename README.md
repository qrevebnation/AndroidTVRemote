# 📺 Android TV Remote (Rust + Slint)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows%20%7C%20macOS-informational)](https://github.com/qrevebnation/AndroidTVRemote)
[![Theme](https://img.shields.io/badge/Theme-Catppuccin%20Mocha-cba6f7.svg)](https://github.com/catppuccin/catppuccin)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

Sebuah aplikasi remote kontrol Google TV / Android TV modern, ultra-ringan, dan berkecepatan tinggi yang ditulis murni dalam bahasa **Rust** dengan antarmuka grafis **Slint**. Dilengkapi fitur **In-App Real-Time 360p Live Screen** (tanpa popup jendela eksternal), enkripsi TLS native, dan tema **Catppuccin Mocha**.

---

## ✨ Fitur Utama

- 📺 **In-App Live Screen Stream (360p @ 30 FPS)**: Menampilkan layar TV secara real-time langsung di dalam aplikasi remote (bukan popup external scrcpy). 
  - **Mode Portrait**: Mini screen 16:9 disematkan di dalam kartu remote.
  - **Mode Fullscreen / Widescreen ($\ge 750$px)**: Tampilan layar lebar di sebelah kiri berdampingan dengan panel kontrol remote di sebelah kanan.
- ⚡ **Zero TV Lag Profile**: Menggunakan profil encode 360p 800 Kbps yang sangat ringan, tidak membuat CPU TV panas atau lag bahkan pada TV tanpa hardware video encoder.
- 🔒 **Protokol TLS Asli (Port 6466)**: Berkomunikasi via protokol resmi Google TV Remote v2 melalui TLS 1.3 dengan pairing PIN 6 karakter.
- 🛡️ **Session Privacy Protection**: Saat tersambung, IP TV otomatis disembunyikan dan digantikan badge `Google TV • Protected Session` serta tombol **Disconnect**.
- 🎯 **Navigasi Presisi (No Double-Click)**: Respon tombol D-pad instan dan stabil tanpa bug penekanan ganda.
- ⌨️ **Keyboard Shortcuts Lengkap**: Navigasi TV langsung menggunakan keyboard laptop/PC Anda.
- 🎨 **Desain Catppuccin Mocha**: Estetika modern dengan warna Mauve, Crust, Mantle, dan Surface yang nyaman di mata.
- 🌐 **Lintas Platform (Cross-Platform)**: Berjalan mulus di **Windows**, **macOS**, dan **Linux**.

---

## ⌨️ Pintasan Keyboard (Shortcuts)

| Tombol Keyboard | Aksi Remote TV |
| :--- | :--- |
| **Panah Atas / Bawah / Kiri / Kanan** | D-Pad Navigasi (Up / Down / Left / Right) |
| **Enter** atau **Spasi** | OK / Select (D-Pad Center) |
| **Escape** atau **Backspace** | Back (Kembali) |
| **H** atau **Home** | Beranda TV (Home) |
| **+** / **=** / **]** | Volume Naik (Volume Up) |
| **-** / **[** | Volume Turun (Volume Down) |
| **M** | Bisukan Suara (Mute) |
| **P** | Tombol Power |
| **K** | Putar / Jeda Media (Play / Pause) |

---

## 📦 Prasyarat Sistem

Aplikasi ini membutuhkan **ADB** (Android Platform Tools) dan **FFmpeg** untuk streaming layar TV berkecepatan tinggi.

### 1. Windows
Buka PowerShell / Terminal:
```powershell
winget install Google.PlatformTools
winget install Gyan.FFmpeg
```

### 2. macOS
Buka Terminal (menggunakan Homebrew):
```bash
brew install android-platform-tools ffmpeg
```

### 3. Linux (Ubuntu / Debian / Arch / Fedora)
- **Ubuntu/Debian**:
  ```bash
  sudo apt update && sudo apt install -y android-tools-adb ffmpeg
  ```
- **Arch Linux**:
  ```bash
  sudo pacman -S android-tools ffmpeg
  ```
- **Fedora**:
  ```bash
  sudo dnf install -y android-tools ffmpeg
  ```

---

## 🚀 Panduan Penggunaan & Pairing Pertama Kali

### Langkah 1: Aktifkan USB/Network Debugging di Android TV
1. Di Google TV / Android TV Anda, buka **Settings** $\rightarrow$ **System** $\rightarrow$ **About**.
2. Klik **Android TV OS build** sebanyak 7 kali berturut-turut hingga muncul pesan *"You are now a developer!"*.
3. Kembali ke **Settings** $\rightarrow$ **System** $\rightarrow$ **Developer Options**.
4. Aktifkan **USB Debugging** dan **Wireless Debugging** (jika ada).
5. Catat alamat IP TV Anda (misalnya `192.168.1.12`) di **Settings** $\rightarrow$ **Network & Internet**.

### Langkah 2: Menghubungkan Aplikasi
1. Buka aplikasi **Android TV Remote**.
2. Masukkan alamat IP TV Anda pada kolom IP di bagian atas, lalu klik **Connect**.
3. Jika menghubungkan untuk pertama kalinya, TV Anda akan memunculkan dialog persetujuan RSA Debugging (pilih **"Always allow from this computer"** $\rightarrow$ **OK**).
4. Layar TV akan menampilkan **6 karakter PIN Pairing**.
5. Masukkan PIN tersebut ke dalam dialog pairing di aplikasi remote, lalu klik **Pair**.
6. Status akan berubah menjadi **Connected** (titik hijau), dan siaran layar TV secara real-time langsung aktif di remote!

---

## 🛠️ Build dari Source (Untuk Developer)

Pastikan Anda telah menginstal **Rust** toolchain ([rustup.rs](https://rustup.rs/)):

```bash
# 1. Clone repository
git clone https://github.com/qrevebnation/AndroidTVRemote.git
cd AndroidTVRemote

# 2. Jalankan aplikasi dalam mode debug
cargo run

# 3. Atau build binary rilis teroptimasi
cargo build --release
```

Binary mandiri hasil build berada di:
- **Windows**: `target\release\atv-remote.exe`
- **macOS / Linux**: `target/release/atv-remote`

---

## 📄 Lisensi

Proyek ini didistribusikan di bawah lisensi **MIT License**.
