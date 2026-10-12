#!/usr/bin/env bash
set -euo pipefail

mode="${1:-}"
runner_temp="${RUNNER_TEMP:-${TMPDIR:-$HOME}}"
if command -v cygpath >/dev/null 2>&1; then
    runner_temp="$(cygpath -u "$runner_temp")"
fi
secret_file=""
helper_file=""
known_hosts_file=""
git_config_file=""
git_helper_file=""
git_private_key_file=""
git_known_hosts_file=""
ssh_kit_url="git"@"github.com:YellowWhiteBlackCat/zhuge-ui-kit.git"

configure() {
    if [[ -z "${SHARED_UI_KIT_READ_KEY:-}" ]]; then
        printf '%s\n' 'SHARED_UI_KIT_READ_KEY is required to resolve the shared UI kit.' >&2
        return 1
    fi

    umask 077
    secret_file="$(mktemp "$runner_temp/taskforest-shared-ui-kit-read-key.XXXXXX")"
    git_config_file="$(mktemp "$runner_temp/taskforest-shared-ui-kit-gitconfig.XXXXXX")"
    chmod 600 "$secret_file" "$git_config_file"
    git_global_config_file="$git_config_file"
    if command -v cygpath >/dev/null 2>&1; then
        git_global_config_file="$(cygpath -m "$git_config_file")"
    fi
    export GIT_CONFIG_GLOBAL="$git_global_config_file"
    export GIT_CONFIG_NOSYSTEM=1

    if [[ "$SHARED_UI_KIT_READ_KEY" == *'PRIVATE KEY'* ]]; then
        known_hosts_file="$(mktemp "$runner_temp/taskforest-shared-ui-kit-known-hosts.XXXXXX")"
        git_private_key_file="$secret_file"
        git_known_hosts_file="$known_hosts_file"
        if command -v cygpath >/dev/null 2>&1; then
            git_private_key_file="$(cygpath -m "$secret_file")"
            git_known_hosts_file="$(cygpath -m "$known_hosts_file")"
        fi
        printf '%s\n' "$SHARED_UI_KIT_READ_KEY" >"$secret_file"
        chmod 600 "$secret_file"
        # These are GitHub's published SSH host keys. The deploy key and host
        # list remain in the runner's temporary directory only.
        cat >"$known_hosts_file" <<'HOST_KEYS'
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
github.com ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=
github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk=
HOST_KEYS
        chmod 600 "$known_hosts_file"
        export GIT_CONFIG_COUNT=2
        export GIT_CONFIG_KEY_0=credential.helper
        export GIT_CONFIG_VALUE_0=
        export GIT_CONFIG_KEY_1="url.\"${ssh_kit_url}\".insteadOf"
        export GIT_CONFIG_VALUE_1="https://github.com/YellowWhiteBlackCat/zhuge-ui-kit.git"
        printf -v key_arg '%q' "$git_private_key_file"
        printf -v hosts_arg '%q' "$git_known_hosts_file"
        export GIT_SSH_COMMAND="ssh -i $key_arg -o IdentitiesOnly=yes -o UserKnownHostsFile=$hosts_arg -o StrictHostKeyChecking=yes -o BatchMode=yes"
    elif [[ "$SHARED_UI_KIT_READ_KEY" == ssh-* || "$SHARED_UI_KIT_READ_KEY" == ecdsa-* ]]; then
        printf '%s\n' 'SHARED_UI_KIT_READ_KEY contains a public key; configure its private read-only key.' >&2
        return 1
    else
        helper_file="$(mktemp "$runner_temp/taskforest-shared-ui-kit-credential-helper.XXXXXX")"
        git_helper_file="$helper_file"
        if command -v cygpath >/dev/null 2>&1; then
            git_helper_file="$(cygpath -u "$helper_file")"
        fi
        printf '%s\n' "$SHARED_UI_KIT_READ_KEY" >"$secret_file"
        chmod 600 "$secret_file"
        cat >"$helper_file" <<'CREDENTIAL_HELPER'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == get ]] || exit 0
protocol= host= path=
while IFS='=' read -r field value; do
    [[ -n "$field" ]] || break
    case "$field" in
        protocol) protocol="$value" ;;
        host) host="$value" ;;
        path) path="$value" ;;
    esac
done
if [[ "$protocol" == https && "$host" == github.com && ( "$path" == YellowWhiteBlackCat/zhuge-ui-kit.git || "$path" == YellowWhiteBlackCat/zhuge-ui-kit ) ]]; then
    printf '%s\n' 'username=x-access-token'
    printf '%s' 'password='
    cat "${TASKFOREST_SHARED_UI_KIT_KEY_FILE:?}"
    printf '\n'
fi
CREDENTIAL_HELPER
        chmod 700 "$helper_file"
        export GIT_CONFIG_COUNT=3
        export GIT_CONFIG_KEY_0=credential.helper
        export GIT_CONFIG_VALUE_0=
        export GIT_CONFIG_KEY_1=credential.helper
        export GIT_CONFIG_VALUE_1="!bash \"$git_helper_file\""
        export GIT_CONFIG_KEY_2=credential.useHttpPath
        export GIT_CONFIG_VALUE_2=true
    fi
}

cleanup() {
    for file in "$secret_file" "$helper_file" "$known_hosts_file" "$git_config_file"; do
        if [[ -n "$file" ]]; then
            rm -f "$file"
        fi
    done
    unset GIT_SSH_COMMAND GIT_CONFIG_COUNT GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM
    unset GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 GIT_CONFIG_KEY_1 GIT_CONFIG_VALUE_1
    unset GIT_CONFIG_KEY_2 GIT_CONFIG_VALUE_2 TASKFOREST_SHARED_UI_KIT_KEY_FILE
}

case "$mode" in
    fetch)
        trap cleanup EXIT
        configure
        unset SHARED_UI_KIT_READ_KEY
        export TASKFOREST_SHARED_UI_KIT_KEY_FILE="$secret_file"
        GIT_TERMINAL_PROMPT=0 CARGO_NET_GIT_FETCH_WITH_CLI=true cargo fetch --locked
        ;;
    *)
        printf 'usage: %s fetch\n' "$0" >&2
        exit 2
        ;;
esac
