#!/usr/bin/env bash
# Build the source and binary RPMs for the committed HEAD.
#
# Needs git, cargo, rpm-build and cargo-rpm-macros, plus the spec's
# BuildRequires. Uncommitted changes are not included, since the source
# tarball comes from `git archive`. The RPMs land in target/rpmbuild/.
set -euo pipefail

cd "$(dirname "$0")/.."

name=noise-generator
version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
top=$PWD/target/rpmbuild
sources=$top/SOURCES

rm -rf "$top"
mkdir -p "$sources"

git archive --format=tar.gz --prefix="$name-$version/" \
    -o "$sources/$name-$version.tar.gz" HEAD

# The RPM build is offline, so ship the crates alongside the source.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cargo vendor --locked --quiet "$tmp/vendor" >/dev/null
tar -C "$tmp" -cJf "$sources/$name-$version-vendor.tar.xz" vendor

rpmbuild -ba --define "_topdir $top" "packaging/$name.spec"
