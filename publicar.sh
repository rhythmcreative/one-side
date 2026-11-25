#!/bin/sh
# Builds One-Side for other people: refuses to go on unless everything the law and
# macOS ask for publishing is there.
#   - legal/PUBLISHER.txt filled in (who publishes it: LSSI art. 10)
#   - DEVELOPER_ID: the "Developer ID Application: …" certificate in the keychain
#   - NOTARY_PROFILE: a profile saved with `xcrun notarytool store-credentials`
# Personal builds keep using ./bundle.sh.
set -e
cd "$(dirname "$0")"
fail() { echo "No se puede publicar: $1" >&2; exit 1; }
[ -f legal/PUBLISHER.txt ] || fail "falta legal/PUBLISHER.txt (copia legal/PUBLISHER.example.txt y rellénalo)."
grep -q COMPLETAR legal/PUBLISHER.txt && fail "legal/PUBLISHER.txt tiene campos sin rellenar."
[ -n "$DEVELOPER_ID" ] || fail "falta DEVELOPER_ID (certificado Developer ID de Apple)."
[ -n "$NOTARY_PROFILE" ] || fail "falta NOTARY_PROFILE (xcrun notarytool store-credentials)."
security find-identity -v -p codesigning | grep -q "$DEVELOPER_ID" || fail "el certificado \"${DEVELOPER_ID}\" no está en el llavero."

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
echo "Lista para publicar: dist/One-Side.zip (firmada, notarizada y con el publicador)."
