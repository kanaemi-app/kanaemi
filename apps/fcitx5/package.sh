#!/usr/bin/env bash
# Build the Fcitx5 add-on and the settings app, and package them as a Debian
# and an RPM package into target/package. Needs nfpm and Fcitx5's
# development files.
#
# Fcitx5 loads the add-on into its own process, so it works with the Fcitx5
# it was built against and later ones of the same library version: build on
# the oldest system the packages are for.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
release="$root/target/release"
out="$root/target/package"
lib=/usr/lib/fcitx5-kanaemi

cargo build --release -p kanaemi-fcitx5 -p kanaemi-settings --manifest-path "$root/Cargo.toml"
addon="$release/libkanaemi_fcitx5.so"

# As for apps/ibus/package.sh.
for binary in "$addon" "$release/kanaemi-settings"; do
  if ldd "$binary" | grep /nix/store >/dev/null; then
    echo "$binary loads libraries from /nix/store; build outside the Nix shell" >&2
    exit 1
  fi
done

# Versions and architectures as apps/ibus/package.sh names them.
version="$("$release/kanaemi-settings" --version)"
package_version="$(perl -pe 's/-(\d+)-g/+$1.g/; tr/-/~/' <<<"$version")"
case "$(uname -m)" in
  x86_64) arch=amd64 rpm_arch=x86_64 ;;
  aarch64) arch=arm64 rpm_arch=aarch64 ;;
  *) echo "no package architecture for $(uname -m)" >&2; exit 1 ;;
esac

mkdir -p "$out"
for conf in addon inputmethod; do
  perl -pe "s|\@LIBDIR\@|$lib|g; s|\@VERSION\@|$version|g" "$root/apps/fcitx5/$conf.conf" >"$out/fcitx5-$conf.conf"
done

# Fcitx5 looks for add-on libraries only in its own folder, which Debian puts
# under the multiarch library folder, as this system names it, and RPM-based
# distributions under lib64.
deb_addons="$(pkg-config --variable=libdir Fcitx5Core)/fcitx5"
rpm_addons=/usr/lib64/fcitx5

# Dependencies as apps/ibus/package.sh works them out.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/debian"
touch "$work/debian/control"
deb_depends="$(cd "$work" && dpkg-shlibdeps -O "$addon" "$release/kanaemi-settings" |
  perl -ne 'if (/^shlibs:Depends=(.*)$/) { print qq(      - "$_"\n) for split /,\s*/, $1 }')"
rpm_depends="$(for binary in "$addon" "$release/kanaemi-settings"; do readelf -d "$binary"; done |
  perl -ne 'print qq(      - "$1()(64bit)"\n) if /\(NEEDED\).*\[(.+)\]/' | sort -u)"

config="$out/fcitx5-nfpm.yaml"
ROOT="$root" RELEASE="$release" OUT="$out" LIB="$lib" VERSION="$package_version" ARCH="$arch" \
  DEB_ADDONS="$deb_addons" RPM_ADDONS="$rpm_addons" DEB_DEPENDS="$deb_depends" RPM_DEPENDS="$rpm_depends" \
  perl -pe 's/\$\{(\w+)\}/$ENV{$1} \/\/ die "$1 is not set\n"/ge' "$root/apps/fcitx5/package/nfpm.yaml" >"$config"
# Named as apps/ibus/package.sh names its files.
nfpm package --config "$config" --packager deb --target "$out/fcitx5-kanaemi_${version}_${arch}.deb"
nfpm package --config "$config" --packager rpm --target "$out/fcitx5-kanaemi-${version}.${rpm_arch}.rpm"
