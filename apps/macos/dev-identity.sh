#!/usr/bin/env bash
# Create the self-signed identity install.sh signs development builds with,
# in the login keychain, so that a rebuild keeps the permissions macOS gave
# the app. Releases are signed by an identity of their own, kept apart from
# this machine (release-identity.sh).
set -euo pipefail

name="Kanaemi Development"
keychain="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$name" "$keychain" >/dev/null 2>&1; then
  echo "\"$name\" is already in the login keychain"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
"$(dirname "$0")/make-identity.sh" "$name" "$work/identity" >/dev/null
security import "$work/identity/identity.p12" -k "$keychain" \
  -P "$(cat "$work/identity/password")" -T /usr/bin/codesign
echo "created \"$name\" in the login keychain"
