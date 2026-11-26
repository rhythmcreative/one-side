#!/bin/sh
# Builds One-Side for other people: refuses to go on unless everything the law and
# macOS ask for publishing is there.
#   - legal/PUBLISHER.txt filled in (who publishes it: LSSI art. 10)
#   - DEVELOPER_ID: the "Developer ID Application: …" certificate in the keychain
#   - NOTARY_PROFILE: a profile saved with `xcrun notarytool store-credentials`
# Personal builds keep using ./bundle.sh.
set -e
cd "$(dirname "$0")"
fail() { echo "Can't publish: $1" >&2; exit 1; }
[ -f legal/PUBLISHER.txt ] || fail "legal/PUBLISHER.txt is missing (copy legal/PUBLISHER.example.txt and fill it in)."
grep -q "FILL IN\|COMPLETAR" legal/PUBLISHER.txt && fail "legal/PUBLISHER.txt still has fields to fill in."
[ -n "$DEVELOPER_ID" ] || fail "DEVELOPER_ID is missing (an Apple Developer ID certificate)."
[ -n "$NOTARY_PROFILE" ] || fail "NOTARY_PROFILE is missing (xcrun notarytool store-credentials)."
security find-identity -v -p codesigning | grep -q "$DEVELOPER_ID" || fail "the certificate \"${DEVELOPER_ID}\" isn't in the keychain."

./bundle.sh
APP=dist/One-Side.app
cp legal/PUBLISHER.txt "$APP/Contents/Resources/"
for b in "$APP/Contents/MacOS/one-side" "$APP/Contents/MacOS/OneSide" "$APP"; do
  codesign --force --options runtime --timestamp --sign "$DEVELOPER_ID" "$b"
done
codesign --verify --strict --deep "$APP"
rm -f dist/One-Side.zip
ditto -c -k --keepParent "$APP" dist/One-Side.zip
xcrun notarytool submit dist/One-Side.zip --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"
spctl --assess --type execute --verbose "$APP"
rm -f dist/One-Side.zip
ditto -c -k --keepParent "$APP" dist/One-Side.zip
echo "Ready to publish: dist/One-Side.zip (signed, notarized and with the publisher)."
