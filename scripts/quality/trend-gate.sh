#!/usr/bin/env bash
# trend-gate.sh — compare one measured value against a committed TSV trend.
#
# The release-binary bloat check used to be a human reading `cargo bloat`;
# this gate makes it a machine check: append every extended-tier run to
# docs/quality/<metric>-trend.tsv and fail when the newest value grows more
# than --limit percent over the previous recorded row. First run seeds the
# file instead of failing.
#
# Usage: scripts/quality/trend-gate.sh --metric <name> --current <value>
#          --trend <tsv-path> [--limit <pct, default 5>]
#   A --current of 0 or empty means "artifact missing" and fails for bloat
#   metrics rather than recording a bogus zero.

set -u

metric=""
current=""
trend_path=""
limit=5

while [[ $# -gt 0 ]]; do
    case "$1" in
    --metric)
        metric="$2"
        shift 2
        ;;
    --current)
        current="$2"
        shift 2
        ;;
    --trend)
        trend_path="$2"
        shift 2
        ;;
    --limit)
        limit="$2"
        shift 2
        ;;
    *)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    esac
done

if [[ -z "$metric" || -z "$trend_path" || -z "$current" ]]; then
    echo "trend-gate: --metric, --current, and --trend are required" >&2
    exit 2
fi

if [[ "$current" == "0" ]]; then
    echo "FAIL trend-gate $metric: measured value is 0 (artifact missing)"
    exit 1
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
absolute_trend="$repo/$trend_path"

if [[ ! -f "$absolute_trend" ]]; then
    printf '# date\tvalue\n%s\t%s\n' "$(date +%F)" "$current" >"$absolute_trend"
    echo "PASS trend-gate $metric: seeded $trend_path with $current (no history yet)"
    exit 0
fi

previous="$(tail -n 1 "$absolute_trend" | cut -f 2)"
if [[ -z "$previous" ]]; then
    printf '%s\t%s\n' "$(date +%F)" "$current" >>"$absolute_trend"
    echo "PASS trend-gate $metric: seeded $current (trend file had no rows)"
    exit 0
fi

limit_bytes=$((previous + previous * limit / 100))
printf '%s\t%s\n' "$(date +%F)" "$current" >>"$absolute_trend"
if ((current > limit_bytes)); then
    echo "FAIL trend-gate $metric: $current > previous $previous (+$limit% cap $limit_bytes)"
    exit 1
fi
echo "PASS trend-gate $metric: $current vs previous $previous (cap $limit_bytes)"
