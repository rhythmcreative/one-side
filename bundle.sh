#!/bin/sh
# Builds One-Side.app into dist/: Rust core and helper, SwiftUI window, icon,
# legal files (licence, third-party notices, privacy) and an ad-hoc signature.
set -e
cd "$(dirname "$0")"
# Runs on macOS 14 and later: C parts of the dependencies too, not just the Swift window.
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build --release
python3 legal/generate.py
mkdir -p target/swift
xcrun swiftc -swift-version 5 -O -parse-as-library -target arm64-apple-macos14.0 \
  -import-objc-header app/bridge.h app/*.swift -L target/release -lone_side \
  -framework CoreFoundation -framework Security -liconv -o target/swift/OneSide 2>&1 | grep -v "built for newer 'macOS' version" || true
test -x target/swift/OneSide
APP=dist/One-Side.app
rm -rf "$APP" dist/icon.iconset
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" dist/icon.iconset
cp target/swift/OneSide "$APP/Contents/MacOS/OneSide"
cp target/release/one-side "$APP/Contents/MacOS/one-side"
# Vulkan driver shim: lets Metal compile KosmicKrisp's deeply nested shaders (vkshim/onesidekk.m).
clang -O2 -dynamiclib -fobjc-arc -arch arm64 -mmacosx-version-min=14.0 -framework Foundation -framework Metal \
    -o "$APP/Contents/Resources/libonesidekk.dylib" vkshim/onesidekk.m
cp LICENSE legal/THIRD_PARTY_NOTICES.txt legal/PRIVACY.md legal/LEGAL.md "$APP/Contents/Resources/"
for s in 16 32 128 256 512; do
  target/release/one-side --make-icon dist/icon.iconset/icon_${s}x${s}.png $s
  target/release/one-side --make-icon dist/icon.iconset/icon_${s}x${s}@2x.png $((s*2))
done
iconutil -c icns dist/icon.iconset -o "$APP/Contents/Resources/AppIcon.icns"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>One-Side</string>
<key>CFBundleDisplayName</key><string>One-Side</string>
<key>CFBundleIdentifier</key><string>app.one-side</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleExecutable</key><string>OneSide</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>LSApplicationCategoryType</key><string>public.app-category.games</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSHumanReadableCopyright</key><string>© 2026 Los autores de One-Side · Licencia MIT</string>
<key>ITSAppUsesNonExemptEncryption</key><false/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
echo "OK $APP"
