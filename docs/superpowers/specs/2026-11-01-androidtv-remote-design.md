# Design Document: Cross-Platform Google TV Remote (Rust + Slint)

**Date:** 2026-11-01  
**Target:** Linux (Wayland/X11), macOS, Windows  
**Location:** `~/Documents/atv-remote`

---

## 1. Overview & Goals

A lightweight, ultra-low-latency, cross-platform desktop remote control for Android TV / Google TV implementing the official **Android TV Remote Service (v2)** protocol from scratch in pure Rust.

### Goals
- **Zero Heavy Runtime Dependencies:** Pure Rust stack (`rustls`, `rcgen`, `prost`, `slint`). No system OpenSSL or Cgo required for cross-platform portability.
- **Ultra-low Resource Consumption:**
  - Memory Footprint: ≤ 25 MB RAM during active session.
  - Latency: < 5ms input-to-wire event delivery.
  - Startup time: < 100ms.
- **Ergonomic Controls:**
  - Virtual Google TV style GUI (circular D-pad, center OK, Back, Home, Volume rocker, Power, Mute, Play/Pause).
  - Global in-window keyboard shortcuts (Arrow keys, Space/Enter, Escape/Backspace, +/-, etc.) with visual feedback.
- **Cross-Platform:** Runs natively on Linux (GNOME/Wayland/X11), macOS, and Windows.
- **Resilient Pairing & Persistence:** One-time PIN pairing, persistent TLS cert storage in standard OS user directories (`dirs::config_dir`).

### Non-Goals
- ADB-based fallback (Android TV Remote v2 is the sole protocol).
- Voice streaming (Microphone input) in v1 (scope kept minimal for rock-solid stability).

---

## 2. Technical Stack & Dependencies

| Layer | Crate / Technology | Role |
|---|---|---|
| **Language** | Rust (Edition 2024 / 2021, rustc ≥ 1.85) | Memory safety, zero-cost abstractions, single binary |
| **GUI Framework** | `slint = "1.9"` | Cross-platform declarative UI, hardware-accelerated (FemtoVG/winit) |
| **Async Runtime** | `tokio = { version = "1", default-features = false, features = ["rt", "net", "time", "sync", "macros"] }` | Lightweight single-thread runtime for TLS/TCP network event loop |
| **TLS & Security** | `rustls = "0.23"` + `rcgen = "0.13"` | Pure-Rust TLS client (custom server cert verifier allowing self-signed peer) + X.509 cert generation |
| **Protobuf Engine**| `prost = "0.13"` + `prost-build` | Zero-allocation protobuf encode/decode |
| **Network Discovery** | `mdns-sd = "0.13"` | Zero-configuration mDNS discovery of `_androidtvremote2._tcp.local` |
| **OS Directories** | `dirs = "6.0"` | Resolves standard OS config folders (XDG, AppData, Library) |
| **Serialization**| `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"` | Config file storage (`config.json`) |

---

## 3. Protocol Specification & Architecture

### 3.1. Wire Framing (Length-Delimited Protobuf)
Every protobuf message over TLS on port 6466 and port 6467 uses standard protobuf varint length prefixing:
```
[ Varint (Byte Length of Payload) ] [ Serialized Protobuf Bytes ]
```
Decoding handles partial packets and stream buffering using `bytes::BytesMut`.

### 3.2. Pairing Flow (Port 6466)
1. **Initiation:** Client connects via TLS to `TV_IP:6466` presenting self-signed client certificate.
2. **Polo Request:** Client sends `OuterMessage` with `PairingRequest { service_name: "atvremote", client_name: "<HostName> Remote" }`.
3. **Polo Ack:** Server responds with `PairingRequestAck`.
4. **Options:** Client sends `Options` offering `ENCODING_TYPE_HEXADECIMAL` with `symbol_length: 6`.
5. **Configuration:** Server sends `Configuration`. Client replies with `ConfigurationAck`.
6. **PIN Display:** TV displays 6-character hex PIN (e.g., `3A8F1C`).
7. **Secret Hash Calculation:**
   - Client extracts RSA Modulus (hex) & Exponent (hex) from client X.509 certificate.
   - Client extracts RSA Modulus (hex) & Exponent (hex) from TV's TLS peer certificate.
   - Computes:
     $$\text{Digest} = \text{SHA256}(\text{client\_modulus} \parallel \text{client\_exp} \parallel \text{server\_modulus} \parallel \text{server\_exp} \parallel \text{PIN}[2..])$$
   - Verification check: `Digest[0] == hex_to_byte(PIN[0..2])`.
8. **Finalize:** Client sends `Secret { secret: Digest }`. Server replies with `SecretAck`.

### 3.3. Remote Session (Port 6467)
1. **Connection:** Client connects via TLS to `TV_IP:6467` with the paired client certificate.
2. **Configuration:**
   - Client sends `RemoteConfigure { code1: 1, device_info: RemoteDeviceInfo { model: "Desktop Remote", vendor: "OpenSource", package_name: "atv.remote", app_version: "1.0.0" } }`.
   - Server returns `RemoteConfigure` confirmation and `RemoteSetActive`.
3. **Key Injection:**
   User input generates `RemoteKeyInject`:
   ```rust
   RemoteMessage {
       remote_key_inject: Some(RemoteKeyInject {
           key_code: RemoteKeyCode::KeycodeDpadCenter as i32,
           direction: RemoteDirection::Short as i32,
       }),
       ..Default::default()
   }
   ```
4. **Keep-Alive:**
   On receiving `RemotePingRequest { val1, val2 }`, client immediately responds with `RemotePingResponse { val1 }`.

---

## 4. User Interface & Key Bindings

### 4.1. Visual Layout (Slint UI)
- **Top Bar:** Connection indicator dot (Green = Connected, Orange = Pairing, Red = Disconnected), TV Hostname/IP, Settings/Reconnect button.
- **Center D-Pad:**
  - Outer ring: Up, Down, Left, Right directional buttons.
  - Inner circle: "OK" / Center select button.
- **Navigation Row:** Back (`←`), Home (`⌂`), Menu (`☰`).
- **Media & Utility Row:** Volume Down (`-`), Volume Mute (`🔇`), Volume Up (`+`), Power (`⏻`).
- **Pairing Modal:** Pop-up dialog prompting for the 6-character PIN when unauthenticated.

### 4.2. Keyboard Controls (In-Window Shortcuts)
| Key | Remote Action |
|---|---|
| `↑` (Up Arrow) | `KEYCODE_DPAD_UP` |
| `↓` (Down Arrow) | `KEYCODE_DPAD_DOWN` |
| `←` (Left Arrow) | `KEYCODE_DPAD_LEFT` |
| `→` (Right Arrow) | `KEYCODE_DPAD_RIGHT` |
| `Enter` / `Space` | `KEYCODE_DPAD_CENTER` |
| `Escape` / `Backspace` | `KEYCODE_BACK` |
| `H` / `Home` | `KEYCODE_HOME` |
| `+` / `=` / `]` | `KEYCODE_VOLUME_UP` |
| `-` / `[` | `KEYCODE_VOLUME_DOWN` |
| `M` | `KEYCODE_VOLUME_MUTE` |
| `P` | `KEYCODE_POWER` |

---

## 5. Storage & State Management

Directory resolved via `dirs::config_dir().join("atv-remote")`:
- `cert.pem`: Client public X.509 certificate.
- `key.pem`: Client RSA private key (2048-bit PKCS#8).
- `config.json`:
  ```json
  {
    "last_tv_ip": "192.168.1.105",
    "tv_name": "Living Room TV",
    "paired": true
  }
  ```

---

## 6. Verification & Quality Gates
1. **Unit Tests:**
   - Varint encoding & decoding test vectors (partial chunks, multiple messages).
   - SHA-256 pairing secret computation test against known vector.
   - Protobuf encoding validation for all keycodes.
2. **Integration / Smoke Test:**
   - Built-in mock Android TV TLS server for automated CI verification.
   - Live smoke test on local Wi-Fi with Android/Google TV.
3. **Memory Profile:**
   - Verified via `/proc/<pid>/status` (VmRSS ≤ 25MB).
