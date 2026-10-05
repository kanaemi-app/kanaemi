#!/usr/bin/env bash
# Write beside each installer in target/package a description of it, which
# the release gathers into its catalog: what the file is for is known here,
# where it was made, rather than guessed later from its name.
#
# Usage: describe-packages.sh <os>
set -euo pipefail

os="$1"
case "${RUNNER_ARCH:-$(uname -m)}" in
  X64 | x86_64 | amd64) arch=x64 ;;
  ARM64 | arm64 | aarch64) arch=arm64 ;;
  *) echo "no architecture name for ${RUNNER_ARCH:-$(uname -m)}" >&2; exit 1 ;;
esac

sha256() {
  if command -v sha256sum >/dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

described=0
for path in target/package/*.pkg target/package/*.msi target/package/*.deb target/package/*.rpm; do
  [ -e "$path" ] || continue
  file="$(basename "$path")"
  printf '{"file":"%s","os":"%s","arch":"%s","format":"%s","size":%s,"sha256":"%s"}\n' \
    "$file" "$os" "$arch" "${file##*.}" "$(wc -c <"$path" | tr -d ' ')" "$(sha256 "$path")" \
    >"$path.json"
  described=$((described + 1))
done
if [ "$described" -eq 0 ]; then
  echo "no installer in target/package" >&2
  exit 1
fi
