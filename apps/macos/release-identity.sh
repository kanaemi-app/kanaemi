#!/usr/bin/env bash
# Make the self-signed identity releases are signed with, for the secrets of
# the GitHub environment "release": MACOS_SIGNING_P12 (identity.p12 in
# base64) and MACOS_SIGNING_PASSWORD. Every release must be signed by the
# same one, or users give the input method its permissions again.
# Development builds are signed by it too (import-identity.sh).
#
#   release-identity.sh <folder to create>
set -euo pipefail

out="$1"
"$(dirname "$0")/make-identity.sh" "Kanaemi Release" "$out"
/usr/bin/base64 -i "$out/identity.p12" >"$out/identity.p12.base64"
cat <<EOF
Register these as secrets of the GitHub environment "release", keep them
where import-identity.sh can be given them, then delete $out:
  MACOS_SIGNING_P12       the content of $out/identity.p12.base64
  MACOS_SIGNING_PASSWORD  the content of $out/password
EOF
