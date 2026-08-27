#!/usr/bin/env bash
# Install, verify, or remove exactly one manifest-authorized polkit helper pair.
#
# This script is deliberately narrower than a package manager. It never creates
# a service, reloads a daemon, edits udev, follows symlinks, or overwrites a
# destination. The manifest and local host receipt are mandatory review inputs.

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
MANIFEST="$REPO_ROOT/docs/system-install-manifest.tsv"
RECEIPT="${TASKFOREST_INSTALL_RECEIPT:-$REPO_ROOT/.private/install-receipts/system-install-host-receipt.tsv}"
RECEIPT_TEMPLATE="$REPO_ROOT/docs/system-install-host-receipt.example.tsv"
HELPER_SRC=""
POLICY_SRC=""
HELPER_CANONICAL_DST=""
POLICY_CANONICAL_DST=""
INSTALL_ROOT="/"
STAGING=0
EXPECTED_OWNER="0:0"
HELPER_DST=""
POLICY_DST=""
HELPER_ID=""
POLICY_ID=""

TEMP_PATHS=()

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    local path
    for path in "${TEMP_PATHS[@]}"; do
        if [[ -e "$path" || -L "$path" ]]; then
            rm -- "$path"
        fi
    done
}

trap cleanup EXIT
trap 'exit 130' INT TERM

usage() {
    printf '%s\n' \
        "usage: $0 {status|verify|install|uninstall} {perf|net|process} [--staging DIR]" \
        "  status    read-only host inspection" \
        "  verify    require both exact artifacts to be installed" \
        "  install   root-only, conflict-safe installation" \
        "  uninstall root-only, receipt-hash-checked removal" \
        "  --staging DIR  test only inside an existing repo .tmp directory"
}

configure_feature() {
    case "$1" in
        perf)
            HELPER_SRC="$REPO_ROOT/target/release/taskmanager-privilege-helper"
            POLICY_SRC="$REPO_ROOT/polkit/com.taskforest.perf-helper.policy.in"
            HELPER_CANONICAL_DST="/usr/libexec/taskmanager-privilege-helper"
            POLICY_CANONICAL_DST="/usr/share/polkit-1/actions/com.taskforest.perf-helper.policy"
            HELPER_ID="POLKIT-PERF-HELPER"
            POLICY_ID="POLKIT-PERF-POLICY"
            ;;
        net)
            HELPER_SRC="$REPO_ROOT/target/release/taskmanager-net-launcher"
            POLICY_SRC="$REPO_ROOT/polkit/com.taskforest.net-launcher.policy.in"
            HELPER_CANONICAL_DST="/usr/libexec/taskmanager-net-launcher"
            POLICY_CANONICAL_DST="/usr/share/polkit-1/actions/com.taskforest.net-launcher.policy"
            HELPER_ID="POLKIT-NET-HELPER"
            POLICY_ID="POLKIT-NET-POLICY"
            ;;
        process)
            HELPER_SRC="$REPO_ROOT/target/release/taskmanager-process-control-helper"
            POLICY_SRC="$REPO_ROOT/polkit/com.taskforest.process-control.policy.in"
            HELPER_CANONICAL_DST="/usr/lib/taskforest-process-control-helper"
            POLICY_CANONICAL_DST="/usr/share/polkit-1/actions/com.taskforest.process-control.policy"
            HELPER_ID="POLKIT-PROCESS-HELPER"
            POLICY_ID="POLKIT-PROCESS-POLICY"
            ;;
        *)
            usage >&2
            exit 64
            ;;
    esac
}

root_path() {
    local canonical="$1"
    if [[ "$INSTALL_ROOT" == "/" ]]; then
        printf '%s' "$canonical"
    else
        printf '%s%s' "$INSTALL_ROOT" "$canonical"
    fi
}

configure_scope() {
    if [[ $# -eq 0 ]]; then
        INSTALL_ROOT="/"
        STAGING=0
        EXPECTED_OWNER="0:0"
    elif [[ $# -eq 2 && "$1" == "--staging" ]]; then
        INSTALL_ROOT="$(cd -- "$2" && pwd -P)"
        [[ "$INSTALL_ROOT" == "$REPO_ROOT/.tmp/"* ]] \
            || die "staging root must be inside this repository's .tmp directory"
        [[ -d "$INSTALL_ROOT" && ! -L "$INSTALL_ROOT" ]] \
            || die "staging root must be an existing real directory"
        STAGING=1
        EXPECTED_OWNER="$(id -u):$(id -g)"
    else
        usage >&2
        exit 64
    fi
    HELPER_DST="$(root_path "$HELPER_CANONICAL_DST")"
    POLICY_DST="$(root_path "$POLICY_CANONICAL_DST")"
}

require_manifest_entry() {
    local id="$1"
    local destination="$2"
    [[ -r "$MANIFEST" ]] || die "manifest is missing: $MANIFEST"
    awk -F '\t' -v expected_id="$id" -v expected_destination="$destination" \
        'NR > 1 && $1 == expected_id && $4 == expected_destination { found = 1 } END { exit !found }' \
        "$MANIFEST" \
        || die "manifest does not authorize $id at $destination"
}

require_inputs() {
    [[ -f "$MANIFEST" && ! -L "$MANIFEST" ]] || die "manifest is not a regular file"
    if [[ "$STAGING" == "0" ]]; then
        [[ -f "$RECEIPT" && ! -L "$RECEIPT" ]] \
            || die "local host receipt is not a regular file: $RECEIPT (copy $RECEIPT_TEMPLATE and record only local host facts)"
    fi
    require_manifest_entry "$HELPER_ID" "$HELPER_CANONICAL_DST"
    require_manifest_entry "$POLICY_ID" "$POLICY_CANONICAL_DST"
    [[ -f "$HELPER_SRC" && ! -L "$HELPER_SRC" ]] || die "release helper is missing: $HELPER_SRC"
    [[ -f "$POLICY_SRC" && ! -L "$POLICY_SRC" ]] || die "policy source is missing: $POLICY_SRC"
    grep -Fq \
        "<annotate key=\"org.freedesktop.policykit.exec.path\">${HELPER_CANONICAL_DST}</annotate>" \
        "$POLICY_SRC" \
        || die "policy annotation does not match the fixed helper destination"
}

ensure_destination_parents() {
    local directory
    for directory in "$(dirname -- "$HELPER_DST")" "$(dirname -- "$POLICY_DST")"; do
        if [[ -d "$directory" && ! -L "$directory" ]]; then
            continue
        fi
        [[ ! -e "$directory" && ! -L "$directory" ]] \
            || die "destination parent is not a real directory: $directory"
        [[ "$directory" == "$(root_path /usr/libexec)" ]] \
            || die "required standard destination directory is missing: $directory"
        require_manifest_entry "POLKIT-LIBEXEC-DIR" "/usr/libexec"
        if [[ "$STAGING" == "1" ]]; then
            install -d -m 0755 "$directory"
        else
            install -d -o 0 -g 0 -m 0755 "$directory"
        fi
        printf 'CREATED-DIRECTORY\t%s\n' "$directory"
    done
}

file_hash() {
    sha256sum "$1" | awk '{ print $1 }'
}

receipt_hash() {
    local id="$1"
    if [[ "$STAGING" == "1" ]]; then
        case "$id" in
            "$HELPER_ID") file_hash "$HELPER_SRC" ;;
            "$POLICY_ID") file_hash "$POLICY_SRC" ;;
            *) return 1 ;;
        esac
        return
    fi
    awk -F '\t' -v expected_id="$id" \
        'NR > 1 && $2 == expected_id && $4 == "present" { print $10; found = 1 } END { if (!found) exit 1 }' \
        "$RECEIPT"
}

metadata_is_expected() {
    local path="$1"
    local mode="$2"
    local actual_mode
    actual_mode="$(stat -c '%a' "$path")"
    if [[ "$actual_mode" != "$mode" ]]; then
        # NTFS under Git Bash/MSYS does not preserve POSIX executable bits in
        # the same way as the Linux CI staging filesystem. Content and file
        # type are still checked; real host transactions always require mode.
        case "$(uname -s)" in
            MINGW* | MSYS* | CYGWIN*) [[ "$STAGING" == "1" ]] || return 1 ;;
            *) return 1 ;;
        esac
    fi
    # A repository-local staging tree is also used by Git Bash on Windows,
    # where NTFS does not expose Unix ownership consistently. Real host
    # transactions still require root:root; isolated tests require the mode
    # and exact bytes while leaving ownership to the staging filesystem.
    [[ "$STAGING" == "1" ]] || [[ "$(stat -c '%u:%g' "$path")" == "$EXPECTED_OWNER" ]]
}

print_status() {
    local id="$1"
    local source="$2"
    local destination="$3"
    local expected
    expected="$(file_hash "$source")"
    if [[ -L "$destination" ]]; then
        printf 'CONFLICT\t%s\tsymlink\t%s\n' "$id" "$destination"
    elif [[ -f "$destination" ]]; then
        local actual
        actual="$(file_hash "$destination")"
        if [[ "$actual" == "$expected" ]]; then
            printf 'PRESENT\t%s\tmode=%s\towner=%s\tsha256=%s\t%s\n' \
                "$id" "$(stat -c '%a' "$destination")" "$(stat -c '%u:%g' "$destination")" "$actual" "$destination"
        else
            printf 'CONFLICT\t%s\tsha256=%s\texpected=%s\t%s\n' "$id" "$actual" "$expected" "$destination"
        fi
    elif [[ -e "$destination" ]]; then
        printf 'CONFLICT\t%s\tnon-regular\t%s\n' "$id" "$destination"
    else
        printf 'ABSENT\t%s\t%s\n' "$id" "$destination"
    fi
}

verify_installed() {
    require_inputs
    local expected_helper expected_policy
    expected_helper="$(file_hash "$HELPER_SRC")"
    expected_policy="$(file_hash "$POLICY_SRC")"
    for pair in \
        "$HELPER_DST:$expected_helper:755" \
        "$POLICY_DST:$expected_policy:644"; do
        IFS=: read -r destination expected mode <<< "$pair"
        [[ -f "$destination" && ! -L "$destination" ]] \
            || die "destination is not a regular file: $destination"
        [[ "$(file_hash "$destination")" == "$expected" ]] \
            || die "content hash mismatch: $destination"
        metadata_is_expected "$destination" "$mode" \
            || die "mode/owner mismatch: $destination (expected root:root $mode)"
    done
    printf 'VERIFY-PASS\t%s\n' "$HELPER_DST"
    printf 'VERIFY-PASS\t%s\n' "$POLICY_DST"
}

install_one() {
    local source="$1"
    local destination="$2"
    local mode="$3"
    local parent
    parent="$(dirname -- "$destination")"
    local expected actual temp
    expected="$(file_hash "$source")"
    if [[ -L "$destination" ]]; then
        die "refusing to follow destination symlink: $destination"
    fi
    if [[ -e "$destination" ]]; then
        [[ -f "$destination" ]] || die "refusing non-regular destination: $destination"
        actual="$(file_hash "$destination")"
        [[ "$actual" == "$expected" ]] \
            || die "refusing to overwrite different content at $destination"
        metadata_is_expected "$destination" "$mode" \
            || die "existing identical file has unexpected mode/owner: $destination"
        printf 'UNCHANGED\t%s\tsha256=%s\n' "$destination" "$expected"
        return
    fi
    temp="$(mktemp "$parent/.taskforest-polkit.XXXXXX")"
    TEMP_PATHS+=("$temp")
    # MSYS coreutils `install` silently truncates an EXISTING destination to
    # zero bytes and still exits 0 (observed on Git Bash 2026-08-27); the
    # copy must target a brand-new inode on every platform.
    rm -f -- "$temp"
    if [[ "$STAGING" == "1" ]]; then
        install -m "$mode" "$source" "$temp"
    else
        install -o 0 -g 0 -m "$mode" "$source" "$temp"
    fi
    # Fail closed: a zero-byte privileged helper must never be published,
    # whatever the copy primitive did.
    [[ -s "$temp" ]] || die "helper copy produced no content: $temp"
    # Same-directory hard-link publication refuses a race winner instead of
    # replacing it, and the EXIT trap removes the staging inode on failure.
    ln -- "$temp" "$destination" \
        || die "destination appeared or could not be published: $destination"
    rm -- "$temp"
    printf 'INSTALLED\t%s\tsha256=%s\n' "$destination" "$expected"
}

preflight_install_one() {
    local source="$1"
    local destination="$2"
    local mode="$3"
    local expected actual
    expected="$(file_hash "$source")"
    if [[ -L "$destination" ]]; then
        die "refusing to follow destination symlink: $destination"
    fi
    if [[ -e "$destination" ]]; then
        [[ -f "$destination" ]] || die "refusing non-regular destination: $destination"
        actual="$(file_hash "$destination")"
        [[ "$actual" == "$expected" ]] \
            || die "refusing to overwrite different content at $destination"
        metadata_is_expected "$destination" "$mode" \
            || die "existing identical file has unexpected mode/owner: $destination"
    fi
}

install_all() {
    if [[ "$STAGING" == "0" ]]; then
        [[ "$(id -u)" == "0" ]] || die "install requires root; run the exact documented sudo command"
    fi
    require_inputs
    ensure_destination_parents
    refuse_package_owned "$HELPER_DST"
    refuse_package_owned "$POLICY_DST"
    preflight_install_one "$HELPER_SRC" "$HELPER_DST" 755
    preflight_install_one "$POLICY_SRC" "$POLICY_DST" 644
    install_one "$HELPER_SRC" "$HELPER_DST" 0755
    install_one "$POLICY_SRC" "$POLICY_DST" 0644
    verify_installed
    printf '%s\n' "Record the two hashes above in the local receipt before declaring installation complete: $RECEIPT"
}

uninstall_one() {
    local id="$1"
    local destination="$2"
    local recorded actual
    recorded="$(receipt_hash "$id")" \
        || die "receipt has no present hash for $id; refuse removal"
    [[ "$recorded" =~ ^[0-9a-f]{64}$ ]] || die "receipt hash is malformed for $id"
    if [[ ! -e "$destination" ]]; then
        printf 'ALREADY-ABSENT\t%s\n' "$destination"
        return
    fi
    [[ -f "$destination" && ! -L "$destination" ]] \
        || die "refusing to remove non-regular or symlink destination: $destination"
    actual="$(file_hash "$destination")"
    [[ "$actual" == "$recorded" ]] \
        || die "refusing removal because installed content changed: $destination"
    rm -- "$destination"
    [[ ! -e "$destination" && ! -L "$destination" ]] \
        || die "destination remains after removal: $destination"
    printf 'REMOVED\t%s\tsha256=%s\n' "$destination" "$actual"
}

uninstall_all() {
    if [[ "$STAGING" == "0" ]]; then
        [[ "$(id -u)" == "0" ]] || die "uninstall requires root; run the exact documented sudo command"
    fi
    require_inputs
    refuse_package_owned "$HELPER_DST"
    refuse_package_owned "$POLICY_DST"
    # Preflight both receipt hashes and both current files before the first rm.
    local helper_recorded policy_recorded
    helper_recorded="$(receipt_hash "$HELPER_ID")" \
        || die "receipt has no present helper hash; refuse removal"
    policy_recorded="$(receipt_hash "$POLICY_ID")" \
        || die "receipt has no present policy hash; refuse removal"
    [[ -f "$HELPER_DST" && ! -L "$HELPER_DST" ]] || die "helper destination is not a regular file"
    [[ -f "$POLICY_DST" && ! -L "$POLICY_DST" ]] || die "policy destination is not a regular file"
    [[ "$(file_hash "$HELPER_DST")" == "$helper_recorded" ]] \
        || die "helper content differs from the recorded receipt"
    [[ "$(file_hash "$POLICY_DST")" == "$policy_recorded" ]] \
        || die "policy content differs from the recorded receipt"
    uninstall_one "$HELPER_ID" "$HELPER_DST"
    uninstall_one "$POLICY_ID" "$POLICY_DST"
}

refuse_package_owned() {
    local destination="$1"
    if [[ "$STAGING" == "0" && -e "$destination" ]] \
        && command -v pacman >/dev/null 2>&1 \
        && pacman -Qqo "$destination" >/dev/null 2>&1; then
        die "package manager owns $destination; use the package transaction"
    fi
}

main() {
    [[ $# -ge 2 ]] || { usage >&2; exit 64; }
    local action="${1:-}"
    local feature="${2:-}"
    shift 2
    configure_feature "$feature"
    configure_scope "$@"
    case "$action" in
        status)
            require_inputs
            print_status "$HELPER_ID" "$HELPER_SRC" "$HELPER_DST"
            print_status "$POLICY_ID" "$POLICY_SRC" "$POLICY_DST"
            ;;
        verify)
            verify_installed
            ;;
        install)
            install_all
            ;;
        uninstall)
            uninstall_all
            ;;
        *)
            usage >&2
            exit 64
            ;;
    esac
}

main "$@"
