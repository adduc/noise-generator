#!/usr/bin/env bash
# Check that Cargo.toml, the spec file and the newest metainfo release all
# carry the same version. With a tag argument (e.g. v1.2.0), check that it
# matches too.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
spec=$(sed -n 's/^Version: *//p' packaging/noise-generator.spec | head -n1)
metainfo=$(sed -n 's/.*<release version="\([^"]*\)".*/\1/p' \
    data/us.jlong.NoiseGenerator.metainfo.xml | head -n1)

status=0
check() {
    if [ "$2" != "$cargo" ]; then
        echo "$1 has version '$2', but Cargo.toml has '$cargo'" >&2
        status=1
    fi
}
check packaging/noise-generator.spec "$spec"
check "the newest metainfo release" "$metainfo"
if [ $# -gt 0 ]; then
    check "tag $1" "${1#v}"
fi

if [ $status -eq 0 ]; then
    echo "version $cargo is consistent"
fi
exit $status
