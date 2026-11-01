#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-1.0.0}"
MODE="${2:-full}"
EMIT_PLIST_TARGET="${3:-}"
APP_NAME="EasyOffice"
BUNDLE_ID="com.aquilaxk.easyoffice"
DIST_DIR="dist"
APP_BUNDLE="${DIST_DIR}/${APP_NAME}.app"

write_info_plist() {
  local target_plist="$1"
  mkdir -p "$(dirname "$target_plist")"
  cat > "$target_plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>ko</string>
  <key>CFBundleExecutable</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>ECMA-376 OOXML and Standard Text Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>org.openxmlformats.wordprocessingml.document</string>
        <string>org.openxmlformats.spreadsheetml.sheet</string>
        <string>org.openxmlformats.presentationml.presentation</string>
        <string>public.comma-separated-values-text</string>
        <string>public.tab-separated-values-text</string>
        <string>public.rtf</string>
        <string>net.daringfireball.markdown</string>
        <string>public.plain-text</string>
      </array>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>docx</string>
        <string>xlsx</string>
        <string>pptx</string>
        <string>csv</string>
        <string>tsv</string>
        <string>rtf</string>
        <string>md</string>
        <string>txt</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Korean Office HWP 5.0 and KS X 6101 OWPML Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>hwp</string>
        <string>hwpx</string>
        <string>hwt</string>
        <string>hwtx</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>ISO/IEC 26300 ODF and OLE2 Binary Office Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>odt</string>
        <string>ods</string>
        <string>odp</string>
        <string>doc</string>
        <string>xls</string>
        <string>ppt</string>
        <string>ppsx</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>ISO 32000 PDF Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>pdf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
EOF
}

if [[ "$MODE" == "--emit-plist-only" ]]; then
  if [[ -z "$EMIT_PLIST_TARGET" ]]; then
    echo "Error: --emit-plist-only requires a target file path" >&2
    exit 1
  fi
  write_info_plist "$EMIT_PLIST_TARGET"
  exit 0
fi

echo "==> [1/4] Building ${APP_NAME} release binary (v${VERSION})..."
swift build -c release
BIN_PATH="$(swift build -c release --show-bin-path)/EasyOfficeApp"

if [[ ! -x "$BIN_PATH" ]]; then
  echo "Error: Built executable not found at $BIN_PATH" >&2
  exit 1
fi

if command -v cargo >/dev/null 2>&1; then
  cargo build --release -p easyoffice-app
fi

echo "==> [2/4] Constructing macOS Application Bundle (${APP_BUNDLE})..."
rm -rf "${DIST_DIR}"
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

cp "$BIN_PATH" "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
chmod +x "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
if [[ -x "target/release/easyoffice-app" ]]; then
  cp "target/release/easyoffice-app" "${APP_BUNDLE}/Contents/MacOS/easyoffice-core-host"
  chmod +x "${APP_BUNDLE}/Contents/MacOS/easyoffice-core-host"
fi
if [[ -d "crates/easyoffice-app/ui" ]]; then
  cp -R "crates/easyoffice-app/ui" "${APP_BUNDLE}/Contents/Resources/ui"
fi
printf "APPL????" > "${APP_BUNDLE}/Contents/PkgInfo"
write_info_plist "${APP_BUNDLE}/Contents/Info.plist"

echo "==> [3/4] Signing and verifying ${APP_BUNDLE}..."
codesign --force --deep --sign - "${APP_BUNDLE}"
codesign --verify --deep --strict "${APP_BUNDLE}"
plutil -lint "${APP_BUNDLE}/Contents/Info.plist"
node scripts/verify-macos-plist.mjs

if [[ "$MODE" == "--verify-only" ]]; then
  echo "==> Bundle verification succeeded (--verify-only). Cleaning temporary dist/ directory."
  rm -rf "${DIST_DIR}"
  exit 0
fi

echo "==> [4/4] Packaging .zip, .dmg, and SHA256SUMS.txt..."
ZIP_NAME="${APP_NAME}-${VERSION}-macos.zip"
DMG_NAME="${APP_NAME}-${VERSION}-macos.dmg"

ditto -c -k --sequesterRsrc --keepParent "${APP_BUNDLE}" "${DIST_DIR}/${ZIP_NAME}"
hdiutil create -volname "${APP_NAME} ${VERSION}" -srcfolder "${APP_BUNDLE}" -ov -format UDZO "${DIST_DIR}/${DMG_NAME}" >/dev/null

(
  cd "${DIST_DIR}"
  shasum -a 256 "${ZIP_NAME}" "${DMG_NAME}" > SHA256SUMS.txt
)

echo "==> Release artifacts ready in ${DIST_DIR}/:"
ls -lh "${DIST_DIR}/${ZIP_NAME}" "${DIST_DIR}/${DMG_NAME}" "${DIST_DIR}/SHA256SUMS.txt"
