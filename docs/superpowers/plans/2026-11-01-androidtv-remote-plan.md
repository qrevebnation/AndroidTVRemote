# Google TV Remote (Rust + Slint) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a high-performance, low-memory (<25MB RAM), cross-platform desktop remote control for Android TV / Google TV in pure Rust with Slint GUI and full keyboard shortcut support.

**Architecture:** A lightweight single-thread Tokio async networking worker communicates with Android TV over TLS using length-delimited Protobuf messages (Polo protocol for pairing on port 6466, RemoteMessage for control on port 6467). The user interface is built with Slint, communicating with the network worker via an unbounded Tokio mpsc channel and capturing keyboard shortcuts directly in-window.

**Tech Stack:** Rust 1.85+, Slint 1.9, Tokio (rt + net), rustls 0.23, rcgen 0.13, prost 0.13, dirs 6.0, bytes 1.x, sha2 0.10, rsa 0.9.

**Spec:** `docs/superpowers/specs/2026-11-01-androidtv-remote-design.md`

## Global Constraints
- Target platforms: Linux (Wayland/X11), macOS, Windows.
- Pure Rust dependencies: No native C/OpenSSL link-time dependencies.
- Memory budget: ≤ 25 MB RSS during active session.
- Hot-path input latency: < 5ms input-to-wire.
- Config directory: Standard OS directory resolved via `dirs::config_dir().join("atv-remote")`.

## Review Focus
- **Malformed Varint Framing:** TV or network drops half of a length prefix; connection must not panic or hang, must cleanly reset buffer or reconnect.
- **TV Certificate Changes / IP Reassignment:** TV reboots or gets a new DHCP IP; application must provide clear "Connection Lost / Reconnecting" state without crashing.
- **Invalid Pairing PIN Entry:** User enters non-hex or mismatched PIN; client-side SHA256 pre-check must reject without crashing or hanging the TLS socket.
- **High-Frequency Key Repeats:** User holds down Arrow keys (rapid repeat events); unbounded channel and single reusable buffer must not balloon memory or drop socket frame alignment.
- **Missing or Corrupted Config File:** `config.json` or `key.pem` corrupted; app must regenerate fresh credentials gracefully rather than failing to start.

---

### Task 1: Project Scaffolding & Configuration

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `src/lib.rs`

**Interfaces:**
- Produces: Base crate compilation with dependencies and release profile optimization (`opt-level = 3`, `lto = true`, `strip = true`).

- [ ] **Step 1: Write Cargo.toml with all necessary pure-Rust dependencies and profiles**
- [ ] **Step 2: Add standard Rust .gitignore**
- [ ] **Step 3: Create minimal src/lib.rs and src/main.rs entrypoint**
- [ ] **Step 4: Verify project compiles cleanly with `cargo check`**
- [ ] **Step 5: Commit**
```bash
git add Cargo.toml .gitignore src/
git commit -m "chore: scaffold atv-remote crate with pure-rust dependencies"
```

---

### Task 2: Protobuf Schema & Build Script

**Files:**
- Create: `proto/polo.proto`
- Create: `proto/remotemessage.proto`
- Create: `build.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces: `pub mod proto` containing `polo` (pairing messages) and `remote` (remote key inject, configure, ping/pong messages).

- [ ] **Step 1: Add official proto files (`polo.proto` and `remotemessage.proto`)**
- [ ] **Step 2: Implement `build.rs` using `prost_build::compile_protos` and Slint build**
- [ ] **Step 3: Expose generated modules in `src/lib.rs`**
- [ ] **Step 4: Verify code generation passes `cargo build` and test structs can be instantiated**
- [ ] **Step 5: Commit**
```bash
git add proto/ build.rs src/
git commit -m "feat(proto): add polo and remotemessage schemas and prost codegen"
```

---

### Task 3: Varint Framing Protocol (Codec)

**Files:**
- Create: `src/codec.rs`
- Test: `tests/codec_test.rs`

**Interfaces:**
- Produces:
  - `pub fn encode_frame<M: prost::Message>(msg: &M, buf: &mut bytes::BytesMut) -> Result<(), std::io::Error>`
  - `pub fn decode_frame<M: prost::Message + Default>(buf: &mut bytes::BytesMut) -> Result<Option<M>, prost::DecodeError>`

- [ ] **Step 1: Write failing test in `tests/codec_test.rs` for partial chunks, single frame, and multiple frames**
- [ ] **Step 2: Run `cargo test --test codec_test` to verify failure**
- [ ] **Step 3: Implement varint length-delimited encode/decode in `src/codec.rs` using `prost::encoding::decode_varint` / `encode_varint`**
- [ ] **Step 4: Run `cargo test --test codec_test` to verify PASS**
- [ ] **Step 5: Commit**
```bash
git add src/codec.rs tests/codec_test.rs
git commit -m "feat(codec): implement zero-alloc varint length-delimited framing"
```

---

### Task 4: Certificate Generation & Config Storage

**Files:**
- Create: `src/config.rs`
- Create: `src/cert.rs`
- Test: `tests/cert_test.rs`

**Interfaces:**
- Produces:
  - `pub struct AppConfig { pub last_tv_ip: Option<String>, pub tv_name: Option<String>, pub paired: bool }`
  - `pub fn load_or_generate_identity() -> Result<(rustls::pki_types::CertificateDer<'static>, rustls::pki_types::PrivateKeyDer<'static>), Box<dyn std::error::Error>>`
  - `pub fn get_config_dir() -> std::path::PathBuf`

- [ ] **Step 1: Write failing test in `tests/cert_test.rs` verifying X.509 cert and RSA key generation and PEM round-trip**
- [ ] **Step 2: Run `cargo test --test cert_test` to verify failure**
- [ ] **Step 3: Implement `src/cert.rs` with `rcgen` (2048-bit RSA, self-signed certificate, 10-year validity)**
- [ ] **Step 4: Implement `src/config.rs` with `dirs::config_dir` resolution and JSON loading/saving**
- [ ] **Step 5: Run `cargo test --test cert_test` to verify PASS**
- [ ] **Step 6: Commit**
```bash
git add src/cert.rs src/config.rs tests/cert_test.rs
git commit -m "feat(crypto): implement self-signed X.509 identity and config management"
```

---

### Task 5: Pairing Engine & Secret Computation (Port 6466)

**Files:**
- Create: `src/pairing.rs`
- Test: `tests/pairing_test.rs`

**Interfaces:**
- Produces:
  - `pub fn calculate_pairing_secret(client_mod: &[u8], client_exp: &[u8], server_mod: &[u8], server_exp: &[u8], pin: &str) -> Result<[u8; 32], PairingError>`
  - `pub async fn start_pairing(tv_ip: &str, cert: &CertificateDer, key: &PrivateKeyDer, pin_rx: tokio::sync::oneshot::Receiver<String>) -> Result<(), PairingError>`

- [ ] **Step 1: Write failing test in `tests/pairing_test.rs` for `calculate_pairing_secret` with known test vector**
- [ ] **Step 2: Run `cargo test --test pairing_test` to verify failure**
- [ ] **Step 3: Implement SHA-256 secret hash calculation and hex validation in `src/pairing.rs`**
- [ ] **Step 4: Implement TLS client handshake (no-verify peer cert) and Polo message exchange for port 6466**
- [ ] **Step 5: Run `cargo test --test pairing_test` to verify PASS**
- [ ] **Step 6: Commit**
```bash
git add src/pairing.rs tests/pairing_test.rs
git commit -m "feat(pairing): implement Polo protocol state machine and secret computation"
```

---

### Task 6: Remote Session Engine (Port 6467)

**Files:**
- Create: `src/remote.rs`
- Test: `tests/remote_test.rs`

**Interfaces:**
- Produces:
  - `pub enum Command { SendKey(RemoteKeyCode, RemoteDirection), Disconnect }`
  - `pub async fn run_remote_session(tv_ip: &str, cert: CertificateDer, key: PrivateKeyDer, cmd_rx: tokio::sync::mpsc::UnboundedReceiver<Command>, status_tx: tokio::sync::mpsc::UnboundedSender<ConnectionState>) -> Result<(), RemoteError>`

- [ ] **Step 1: Write failing test in `tests/remote_test.rs` simulating mock server receiving configure and key inject frames**
- [ ] **Step 2: Run `cargo test --test remote_test` to verify failure**
- [ ] **Step 3: Implement `src/remote.rs`: TLS connection on port 6467, `RemoteConfigure` handshake, Ping/Pong responder, and `RemoteKeyInject` loop**
- [ ] **Step 4: Run `cargo test --test remote_test` to verify PASS**
- [ ] **Step 5: Commit**
```bash
git add src/remote.rs tests/remote_test.rs
git commit -m "feat(remote): implement active remote session with ping/pong and key injection"
```

---

### Task 7: Slint UI Design & In-Window Keyboard Mapping

**Files:**
- Create: `ui/appwindow.slint`
- Modify: `build.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces:
  - Slint `AppWindow` with D-pad, Center OK, Back, Home, Volume buttons, Power button, IP input & connection status dot.
  - Global `FocusScope` capturing `Key.UpArrow`, `Key.DownArrow`, `Key.LeftArrow`, `Key.RightArrow`, `Key.Return`, `Space`, `Escape`, `+`, `-`, `M`, `P`.
  - Callback: `callback send_key(string)` and `callback pair_with_ip(string, string)`.

- [ ] **Step 1: Create `ui/appwindow.slint` with modern dark theme Google TV remote layout and modal PIN dialog**
- [ ] **Step 2: Add keyboard shortcut bindings inside `FocusScope`**
- [ ] **Step 3: Compile UI via `build.rs` and verify clean build**
- [ ] **Step 4: Run test binary to verify window loads and button clicks/keys fire callbacks**
- [ ] **Step 5: Commit**
```bash
git add ui/ build.rs src/main.rs
git commit -m "feat(ui): create Google TV remote Slint layout and keyboard navigation"
```

---

### Task 8: Integration & Tokio Channel Wiring

**Files:**
- Modify: `src/main.rs`
- Create: `src/app.rs`

**Interfaces:**
- Connects Slint GUI event callbacks to Tokio background network worker.
- Manages auto-reconnect and pairing dialog transitions.

- [ ] **Step 1: Implement `src/app.rs` orchestrating state transitions: Unpaired -> Pairing -> Connected -> Disconnected**
- [ ] **Step 2: Wire Slint callbacks (`send_key`, `submit_pin`, `connect_ip`) to Tokio channels in `src/main.rs`**
- [ ] **Step 3: Verify clean build and non-blocking GUI event loop**
- [ ] **Step 4: Commit**
```bash
git add src/app.rs src/main.rs
git commit -m "feat(app): wire Slint UI to Tokio async network engine"
```

---

### Task 9: Smoke Test, Verification & Memory Check

**Files:**
- Create: `tests/mock_tv.rs`
- Modify: `Cargo.toml` (release profile strip & lto)

**Interfaces:**
- Produces: Automated end-to-end smoke test against mock Android TV TLS server and memory RSS verification.

- [ ] **Step 1: Implement `tests/mock_tv.rs` that listens on mock ports 6466 & 6467 and asserts pairing + key delivery**
- [ ] **Step 2: Run `cargo test --all` and verify all tests PASS**
- [ ] **Step 3: Build release binary with `cargo build --release`**
- [ ] **Step 4: Measure VmRSS and binary size to guarantee <25MB RAM and <15MB binary**
- [ ] **Step 5: Commit**
```bash
git add tests/mock_tv.rs Cargo.toml
git commit -m "test: add mock TV e2e smoke test and verify memory footprint"
```
