#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="${DIR}/target/release/atv-remote"

if [ ! -f "$BIN_PATH" ]; then
    echo "Building release binary..."
    cargo build --release
fi

echo "Installing binary to ~/.local/bin/..."
mkdir -p "$HOME/.local/bin"
cp "$BIN_PATH" "$HOME/.local/bin/atv-remote"
chmod +x "$HOME/.local/bin/atv-remote"

echo "Installing application icons..."
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps" \
         "$HOME/.local/share/icons/hicolor/512x512/apps" \
         "$HOME/.local/share/icons/hicolor/256x256/apps" \
         "$HOME/.local/share/icons/hicolor/128x128/apps" \
         "$HOME/.local/share/icons/hicolor/64x64/apps"

cp "${DIR}/assets/icon.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/atv-remote.svg"
cp "${DIR}/assets/icon.png" "$HOME/.local/share/icons/hicolor/512x512/apps/atv-remote.png"
cp "${DIR}/assets/icon-256.png" "$HOME/.local/share/icons/hicolor/256x256/apps/atv-remote.png"
cp "${DIR}/assets/icon-128.png" "$HOME/.local/share/icons/hicolor/128x128/apps/atv-remote.png"
cp "${DIR}/assets/icon-64.png" "$HOME/.local/share/icons/hicolor/64x64/apps/atv-remote.png"

echo "Installing desktop launcher..."
mkdir -p "$HOME/.local/share/applications"
cat << EOF > "$HOME/.local/share/applications/atv-remote.desktop"
[Desktop Entry]
Name=Android TV Remote
GenericName=Google TV Remote Control
Comment=Desktop remote control with real-time in-app screen for Android TV and Google TV
Exec=$HOME/.local/bin/atv-remote
Icon=atv-remote
Terminal=false
Type=Application
Categories=Utility;AudioVideo;Network;RemoteAccess;
Keywords=android;tv;remote;google;stream;
StartupWMClass=atv-remote
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$HOME/.local/share/applications"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo "Android TV Remote installed successfully!"
echo "You can now launch it from your Applications menu or by typing 'atv-remote'."
