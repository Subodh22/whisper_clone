#!/usr/bin/env bash
set -e

APP_NAME="VoxType"
BUNDLE="$APP_NAME.app"
BINARY_NAME="voxtype"

echo "==> Building release binary..."
cargo build --release

echo "==> Creating .app bundle..."
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS"
mkdir -p "$BUNDLE/Contents/Resources"

cp "target/release/$BINARY_NAME" "$BUNDLE/Contents/MacOS/$APP_NAME"

cat > "$BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>com.voxtype.app</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>VoxType needs microphone access to record your voice for dictation.</string>
    <key>NSAppleEventsUsageDescription</key>
    <string>VoxType needs Accessibility access to type transcribed text into other apps.</string>
</dict>
</plist>
EOF

echo "==> Packaging into DMG..."
if command -v hdiutil &>/dev/null; then
    DMG_DIR=$(mktemp -d)
    cp -r "$BUNDLE" "$DMG_DIR/"
    ln -s /Applications "$DMG_DIR/Applications"
    hdiutil create -volname "$APP_NAME" \
        -srcfolder "$DMG_DIR" \
        -ov -format UDZO \
        "$APP_NAME.dmg" 2>/dev/null
    rm -rf "$DMG_DIR"
    echo ""
    echo "Done! Created:"
    echo "  $BUNDLE        (drag to /Applications)"
    echo "  $APP_NAME.dmg  (share this)"
else
    echo ""
    echo "Done! Created $BUNDLE (drag to /Applications)"
fi
