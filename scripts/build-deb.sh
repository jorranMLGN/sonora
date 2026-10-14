#!/bin/bash
# Packages an already built release executable as a .deb with cargo-deb, from the
# [package.metadata.deb] section of crates/sonora/Cargo.toml.
#
#   scripts/build-deb.sh <target> <version> <out dir>
#
# <target> is the triple the executable was built for, so target/<target>/release/sonora must
# exist. <version> names the file: a tag (v1.2.3, v1.2.3-beta.1) also becomes the package
# version, anything else (a short commit) gets the Cargo version plus a git suffix that sorts
# after it and after every earlier commit.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <target> <version> <out dir>" >&2
  exit 1
fi

TARGET="$1"
VERSION="$2"
OUT="$3"
ROOT="$(dirname "$(readlink -f "$0")")/.."
cd "$ROOT"

case "$TARGET" in
  x86_64-*) ARCH=amd64 ;;
  aarch64-*) ARCH=arm64 ;;
  *)
    echo "no Debian architecture for $TARGET" >&2
    exit 1
    ;;
esac

cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
case "$VERSION" in
  v[0-9]*) number="${VERSION#v}" ;;
  *) number="$cargo_version+git$(git log -1 --format=%cd --date=format:%Y%m%d%H%M%S).$(git rev-parse --short=7 HEAD)" ;;
esac
# Debian sorts ~ before the release it precedes, which is what a SemVer prerelease means.
deb_version="${number//-/\~}"

# The Flatpak's metainfo, versioned the way its manifest does it, and launched through the
# desktop entry this package installs under its plain name.
mkdir -p target/deb
sed \
  -e "s/@VERSION@/$number/" \
  -e "s/@DATE@/$(date -I)/" \
  -e "s#<launchable type=\"desktop-id\">.*</launchable>#<launchable type=\"desktop-id\">sonora.desktop</launchable>#" \
  flatpak/io.github.nolight132.sonora.metainfo.xml \
  > target/deb/io.github.nolight132.sonora.metainfo.xml

# One file per icon pack, named the way the AppImage and the macOS bundle name them.
rm -rf target/deb/icons
mkdir -p target/deb/icons
cp assets/icons/LICENSE target/deb/icons/LICENSE
for licence in assets/icons/*/LICENSE; do
  pack="$(basename "$(dirname "$licence")")"
  cp "$licence" "target/deb/icons/LICENSE.$pack"
done

mkdir -p "$OUT"
cargo deb \
  --package sonora \
  --target "$TARGET" \
  --no-build \
  --no-strip \
  --deb-version "$deb_version" \
  --output "$OUT/sonora-$VERSION-$ARCH.deb"
