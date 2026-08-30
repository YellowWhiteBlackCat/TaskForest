#!/usr/bin/env bash
# Build the TaskForest .msi on a native Windows toolchain (git-bash).
#
# The WiX v7 SDK arrives as a dotnet global tool; the staged exes come from
# scripts/build-frontend-binaries.sh release (GPUI shape). This wrapper only
# maps the stage directory, the numeric MSI version, and the
# output path onto `wix build` preprocessor variables.
#
# MSI ProductVersion is numeric x.y.z only: pass the release version with any
# prerelease suffix already stripped (a v0.1.0-rc5 tag packages as 0.1.0 with
# AllowSameVersionUpgrades covering the later final install).
#
# Usage: packaging/windows/build-msi.sh STAGE_DIR NUMERIC_VERSION MSI_ARCH OUTPUT_MSI [FULL_VERSION]
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)

if [[ $# -ne 4 && $# -ne 5 ]]; then
    echo "usage: $0 STAGE_DIR NUMERIC_VERSION MSI_ARCH OUTPUT_MSI [FULL_VERSION]" >&2
    exit 2
fi
stage=$1
version=$2
msi_arch=$3
output=$4
# The full Cargo version (prerelease suffix included, e.g. 0.1.0-rc5) rides in
# the MSI summary metadata, the ARP comments, and the installed
# Software\TaskForest\Version registry value; ProductVersion itself stays
# numeric because Windows Installer rejects anything else.
full_version=${5:-$version}
icon_path=$(cygpath -w "$script_dir/taskmanager.ico" 2>/dev/null \
    || printf '%s' "$script_dir/taskmanager.ico")

[[ -f "$stage/taskforest-g.exe" ]] \
    || { echo "build-msi: $stage is missing the GPUI exe" >&2; exit 1; }
[[ -s "$script_dir/taskmanager.ico" ]] \
    || { echo "build-msi: TaskForest Windows icon is missing or empty" >&2; exit 1; }
command -v wix >/dev/null || { echo "build-msi: wix not installed (dotnet tool install --global wix)" >&2; exit 1; }

case "$version" in
    *[!0-9.]* | *..* | .* | *.) echo "build-msi: NUMERIC_VERSION must be x.y.z, got '$version'" >&2; exit 1 ;;
esac

case "$msi_arch" in
    x64|arm64) ;;
    *) echo "build-msi: MSI_ARCH must be x64 or arm64, got '$msi_arch'" >&2; exit 1 ;;
esac

# Third-party notices ship inside the MSI next to the license: the release
# exe embeds OFL fonts and links the dependency closure, so their full terms
# must ride with the installer. Generated from the Cargo.lock graph at build
# time; the staged file is never hand-edited.
py=$(command -v python3 || command -v python || true)
[[ -n "$py" ]] || { echo "build-msi: python3 or python required for THIRD-PARTY-NOTICES.txt" >&2; exit 1; }
"$py" "$repo/scripts/gen_third_party_notices.py" "$stage/THIRD-PARTY-NOTICES.txt"

wix build -acceptEula wix7 -arch "$msi_arch" \
    -d "StageDir=$(cygpath -w "$stage" 2>/dev/null || printf '%s' "$stage")" \
    -d "IconPath=$icon_path" \
    -d "ProductVersion=$version" \
    -d "FullVersion=$full_version" \
    -bindvariable "WixUILicenseRtf=$(cygpath -w "$script_dir/license.rtf" 2>/dev/null || printf '%s' "$script_dir/license.rtf")" \
    -ext WixToolset.UI.wixext/7.0.0 \
    -o "$output" \
    "$script_dir/taskforest.wxs"

[[ -f "$output" ]] || { echo "build-msi: $output was not produced" >&2; exit 1; }
echo "build-msi: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
