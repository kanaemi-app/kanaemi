#!/usr/bin/env bash
# Build the IBus engine and the settings app, and package them as a Debian
# and an RPM package into target/package. Needs nfpm.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
release="$root/target/release"
out="$root/target/package"

cargo build --release -p kanaemi-ibus -p kanaemi-settings --manifest-path "$root/Cargo.toml"

# A build in the Nix development shell loads its libraries from /nix/store,
# which other systems do not have.
# grep reads to the end: with -q it could stop ldd early, and pipefail would
# take the broken pipe for "not found".
for binary in "$release/kanaemi-ibus" "$release/kanaemi-settings"; do
  if ldd "$binary" | grep /nix/store >/dev/null; then
    echo "$binary loads libraries from /nix/store; build outside the Nix shell" >&2
    exit 1
  fi
done

# The version kanaemi_core::VERSION reports, as the settings app prints it.
version="$("$release/kanaemi-settings" --version)"
# RPM takes no hyphen in a version, and both formats order 0.1.0+3.gabc1234
# after 0.1.0 as the build three commits past it should be. A pre-release
# tag's hyphen becomes a tilde, which both order before the release itself
# (0.1.0~rc.1 before 0.1.0); Debian would take a hyphen for a revision.
package_version="$(perl -pe 's/-(\d+)-g/+$1.g/; tr/-/~/' <<<"$version")"
case "$(uname -m)" in
  x86_64) arch=amd64 rpm_arch=x86_64 ;;
  aarch64) arch=arm64 rpm_arch=aarch64 ;;
  *) echo "no package architecture for $(uname -m)" >&2; exit 1 ;;
esac

mkdir -p "$out"
component="$out/kanaemi.xml"
perl -pe "s|\@LIBDIR\@|/usr/lib/ibus-kanaemi|g; s|\@VERSION\@|$version|g" "$root/apps/ibus/kanaemi.xml" >"$component"

# Debian depends on the packages that hold the libraries the binaries link
# to, as this system names them; dpkg-shlibdeps works in a debian folder.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/debian"
touch "$work/debian/control"
deb_depends="$(cd "$work" && dpkg-shlibdeps -O "$release/kanaemi-ibus" "$release/kanaemi-settings" |
  perl -ne 'if (/^shlibs:Depends=(.*)$/) { print qq(      - "$_"\n) for split /,\s*/, $1 }')"
# RPM depends on the libraries themselves, whatever package holds them, so
# one package fits distributions that name their packages differently.
rpm_depends="$(for binary in "$release/kanaemi-ibus" "$release/kanaemi-settings"; do readelf -d "$binary"; done |
  perl -ne 'print qq(      - "$1()(64bit)"\n) if /\(NEEDED\).*\[(.+)\]/' | sort -u)"

# nfpm expands variables in only some of its fields, so they are filled in here.
config="$out/nfpm.yaml"
ROOT="$root" RELEASE="$release" COMPONENT="$component" VERSION="$package_version" ARCH="$arch" \
  DEB_DEPENDS="$deb_depends" RPM_DEPENDS="$rpm_depends" \
  perl -pe 's/\$\{(\w+)\}/$ENV{$1} \/\/ die "$1 is not set\n"/ge' "$root/apps/ibus/package/nfpm.yaml" >"$config"
# The files are named after the version as it is, not the package version:
# a release renames an uploaded file with a tilde, and the catalog would then
# name a file the release does not hold.
nfpm package --config "$config" --packager deb --target "$out/ibus-kanaemi_${version}_${arch}.deb"
nfpm package --config "$config" --packager rpm --target "$out/ibus-kanaemi-${version}.${rpm_arch}.rpm"
