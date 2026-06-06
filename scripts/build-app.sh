#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# build-app.sh — Build Bol.app bundle for macOS
#
# Usage:
#   ./scripts/build-app.sh           # small.en model (~466 MB)
#   ./scripts/build-app.sh medium    # medium.en model (~1.5 GB)
#   ./scripts/build-app.sh sign      # build + ad-hoc sign (for local testing)
#
# The resulting Bol.app can be dragged to /Applications.
# For distribution / notarization, sign with a real Developer ID certificate.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

APP_NAME="Bol"
BUNDLE_ID="com.bol.dictation"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
APP_DIR="${APP_NAME}.app"
BINARY_NAME="bol"

SIGN=false
MEDIUM=false
for arg in "$@"; do
  case "$arg" in
    medium) MEDIUM=true ;;
    sign)   SIGN=true ;;
  esac
done

# ── Build ──────────────────────────────────────────────────────────────────
echo "▶ Building ${APP_NAME} v${VERSION}..."
if $MEDIUM; then
  cargo build --release --features medium
  echo "   Model: medium.en"
else
  cargo build --release
  echo "   Model: small.en"
fi

# ── Bundle structure ────────────────────────────────────────────────────────
echo "▶ Assembling ${APP_DIR}..."
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

cp "target/release/${BINARY_NAME}" "${APP_DIR}/Contents/MacOS/${BINARY_NAME}"
cp "bundle/Info.plist"              "${APP_DIR}/Contents/Info.plist"

# Copy icon if available (generate with scripts/gen-icon.sh or use your own)
if [ -f "assets/AppIcon.icns" ]; then
  cp "assets/AppIcon.icns" "${APP_DIR}/Contents/Resources/AppIcon.icns"
  echo "   Icon: assets/AppIcon.icns"
else
  echo "   Icon: none (add assets/AppIcon.icns for a real icon)"
fi

# ── Code signing ────────────────────────────────────────────────────────────
if $SIGN; then
  if security find-identity -v -p codesigning | grep -q "Developer ID"; then
    IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID" | head -1 | awk -F'"' '{print $2}')
    echo "▶ Signing with: ${IDENTITY}"
    codesign --force --deep --options runtime \
      --entitlements bundle/entitlements.plist \
      --sign "${IDENTITY}" \
      "${APP_DIR}"
  else
    echo "▶ No Developer ID found — ad-hoc signing (local use only)"
    codesign --force --deep --sign - "${APP_DIR}"
  fi
else
  echo "▶ Skipping signing (pass 'sign' to sign)"
fi

# ── Summary ─────────────────────────────────────────────────────────────────
BUNDLE_SIZE=$(du -sh "${APP_DIR}" | cut -f1)
echo ""
echo "✅ ${APP_DIR} (${BUNDLE_SIZE})"
echo ""
echo "   To install:  cp -r ${APP_DIR} /Applications/"
echo "   To run now:  open ${APP_DIR}"
echo ""
echo "   First launch: grant Microphone + Accessibility in System Settings."
echo "   Accessibility: System Settings → Privacy & Security → Accessibility"
