#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-1.0.0}"
MODE="${2:-full}"
APP_NAME="EasyOffice"
BUNDLE_ID="com.aquilaxk.easyoffice"
DIST_DIR="dist"
APP_BUNDLE="${DIST_DIR}/${APP_NAME}.app"

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

cat > "${APP_BUNDLE}/Contents/Info.plist" <<EOF
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
      <string>Office &amp; HWP Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>org.openxmlformats.wordprocessingml.document</string>
        <string>org.openxmlformats.spreadsheetml.sheet</string>
        <string>org.openxmlformats.presentationml.presentation</string>
        <string>com.adobe.pdf</string>
        <string>public.comma-separated-values-text</string>
        <string>net.daringfireball.markdown</string>
        <string>public.plain-text</string>
      </array>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>docx</string>
        <string>doc</string>
        <string>xlsx</string>
        <string>xls</string>
        <string>pptx</string>
        <string>ppt</string>
        <string>ppsx</string>
        <string>hwp</string>
        <string>hwpx</string>
        <string>hwt</string>
        <string>hwtx</string>
        <string>pdf</string>
        <string>odt</string>
        <string>ods</string>
        <string>odp</string>
        <string>csv</string>
        <string>tsv</string>
        <string>md</string>
        <string>txt</string>
        <string>rtf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
EOF

echo "==> [3/4] Signing and verifying ${APP_BUNDLE}..."
codesign --force --deep --sign - "${APP_BUNDLE}"
codesign --verify --deep --strict "${APP_BUNDLE}"
plutil -lint "${APP_BUNDLE}/Contents/Info.plist"

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
