#!/bin/sh
# Create a local self-signed code-signing identity so macOS privacy grants
# (Accessibility, Input Monitoring) survive rebuilds. Ad-hoc signatures change
# with every build and silently invalidate those grants.
set -eu
name="${ORBWEAVER_SIGN_IDENTITY:-Orbweaver Local Signing}"
keychain="$HOME/Library/Keychains/login.keychain-db"
if security find-identity -v -p codesigning | grep -q "\"$name\""; then
    printf 'Identity "%s" already exists.\n' "$name"
    exit 0
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat > "$work/ext.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = $name
[v3]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -sha256 \
    -config "$work/ext.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" >/dev/null 2>&1
security import "$work/key.pem" -k "$keychain" -T /usr/bin/codesign -T /usr/bin/security >/dev/null
security import "$work/cert.pem" -k "$keychain" -T /usr/bin/codesign >/dev/null
# Trust it for code signing only. This may ask for your login password.
security add-trusted-cert -p codeSign -k "$keychain" "$work/cert.pem"
security find-identity -v -p codesigning | grep "\"$name\"" || {
    printf 'Identity was imported but is not yet valid for code signing.\n' >&2
    exit 1
}
printf 'Created identity "%s". scripts/build-app.sh will use it automatically.\n' "$name"
