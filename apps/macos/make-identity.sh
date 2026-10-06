#!/usr/bin/env bash
# Make a self-signed code signing identity: identity.p12 and the password
# that opens it, written into a new folder. macOS keeps the permissions it
# gave an app when a rebuild is signed by the same certificate.
#
#   make-identity.sh <certificate name> <folder to create>
set -euo pipefail

name="$1"
out="$2"
mkdir "$out"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cat >"$work/openssl.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
# The system's LibreSSL writes a PKCS#12 file the keychain can import;
# OpenSSL 3 encrypts it in a way the keychain cannot read.
openssl=/usr/bin/openssl
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 7300 \
  -config "$work/openssl.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
"$openssl" rand -hex 24 >"$out/password"
"$openssl" pkcs12 -export -passout "file:$out/password" \
  -inkey "$work/key.pem" -in "$work/cert.pem" -out "$out/identity.p12"
"$openssl" x509 -in "$work/cert.pem" -noout -fingerprint -sha256
