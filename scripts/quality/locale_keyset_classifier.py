#!/usr/bin/env python3
"""Classify a ``locales/*`` diff as key-set-only or value-affecting.

``scripts/quality/ui-evidence-route.sh`` must not demand four fresh pixel
receipts for a locale edit that cannot alter any painted string.  The two edits
it has to separate are:

* **key-set-only** — whole keys were added or removed for every catalog at once
  and no key that still exists had its value edited.  No painted string can
  change, so the headless interaction channel is sufficient.
* **value-affecting** — a translated value changed, or a catalog's shape or its
  cross-catalog key relationship moved.  The catalogs are ``include_str!``
  embedded by the shared application layer every product links, so such an edit
  can repaint every frontend and keeps all four pixel receipts.

This module owns the content-level judgment so the bash route never parses JSON.
It fails closed: an unparsable catalog, a catalog missing on either side, a
changed catalog file set, a non-flat or non-string catalog, a duplicate key, or
any ambiguity classifies as ``value-affecting`` -- the stricter requirement.

Exact rule, for the changed catalogs under ``locales/``:

    key-set-only  iff every catalog parses on both sides as a flat
                  ``string -> string`` object, the catalog file set is identical
                  at base and current, no key present on both sides changed
                  value, and the cross-catalog *key presence relationship* is
                  preserved:

                    * a key that survives keeps its exact set of catalogs;
                    * an added key is present in every catalog;
                    * a removed key was present in every catalog.

                  The last two clauses forbid an asymmetric add/remove (which
                  would also fail the catalog-symmetry nextest gate); the
                  surviving-relationship clause tolerates the repository's one
                  deliberate asymmetric fallback fixture without naming it.

    value-affecting otherwise.

Usage::

    python3 scripts/quality/locale_keyset_classifier.py \\
        --repo-root . --base <ref> locales/en.json locales/zh.json
    python3 scripts/quality/locale_keyset_classifier.py --self-test

The classifier prints ``key-set-only`` or ``value-affecting`` on stdout and
exits 0 in both cases; the route treats any other output (or a nonzero exit) as
``value-affecting``.  ``--self-test`` exits 1 when a built-in case mismatches.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Iterable, Mapping

KEY_SET_ONLY = "key-set-only"
VALUE_AFFECTING = "value-affecting"

# Only a JSON catalog directly under ``locales/`` is a catalog.  A changed path
# that does not match (a nested file, a non-JSON artifact) is ambiguous and
# therefore value-affecting.
CATALOG_RE = re.compile(r"^locales/[^/]+\.json$")


class _DuplicateKey(ValueError):
    """A JSON object declared the same key more than once."""


def _reject_duplicate_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    seen: dict[str, object] = {}
    for key, value in pairs:
        if key in seen:
            raise _DuplicateKey(key)
        seen[key] = value
    return seen


def parse_catalog(raw: str) -> dict[str, str] | None:
    """Return a flat ``str -> str`` catalog, or ``None`` on any shape doubt."""

    try:
        parsed = json.loads(raw, object_pairs_hook=_reject_duplicate_pairs)
    except (ValueError, TypeError):
        return None
    if not isinstance(parsed, dict):
        return None
    for key, value in parsed.items():
        if not isinstance(key, str) or not isinstance(value, str):
            return None
    return parsed


def _presence(key: str, catalogs: Iterable[str], keysets: Mapping[str, dict[str, str]]) -> frozenset[str]:
    return frozenset(name for name in catalogs if key in keysets[name])


def classify_text(
    base_texts: Mapping[str, str],
    current_texts: Mapping[str, str],
    changed: Iterable[str],
) -> str:
    """Classify raw catalog contents; ``value-affecting`` on any doubt."""

    changed = list(changed)
    if not changed:
        return VALUE_AFFECTING
    if set(base_texts) != set(current_texts):
        return VALUE_AFFECTING
    catalogs = sorted(base_texts)
    # A changed path that is not one of the parsed catalogs is ambiguous.
    if any(name not in base_texts for name in changed):
        return VALUE_AFFECTING

    parsed_base = {name: parse_catalog(text) for name, text in base_texts.items()}
    parsed_current = {name: parse_catalog(text) for name, text in current_texts.items()}
    if any(value is None for value in parsed_base.values()):
        return VALUE_AFFECTING
    if any(value is None for value in parsed_current.values()):
        return VALUE_AFFECTING

    # No surviving key may have changed value.  A whole-key removal is fine.
    for name in catalogs:
        before = parsed_base[name]
        after = parsed_current[name]
        for key, value in before.items():
            if key in after and after[key] != value:
                return VALUE_AFFECTING

    # The cross-catalog presence relationship must be preserved.  This tolerates
    # a pre-existing asymmetric fixture but forbids introducing or widening one.
    every_catalog = frozenset(catalogs)
    universe: set[str] = set()
    for keyset in parsed_base.values():
        universe.update(keyset)
    for keyset in parsed_current.values():
        universe.update(keyset)
    for key in universe:
        before_sig = _presence(key, catalogs, parsed_base)
        after_sig = _presence(key, catalogs, parsed_current)
        if before_sig and after_sig:
            if before_sig != after_sig:
                return VALUE_AFFECTING
        elif after_sig:
            if after_sig != every_catalog:
                return VALUE_AFFECTING
        elif before_sig != every_catalog:
            return VALUE_AFFECTING

    return KEY_SET_ONLY


def _git(repo_root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )


def _base_catalog_names(repo_root: Path, base: str) -> list[str]:
    output = _git(repo_root, "ls-tree", "-r", "--name-only", base, "--", "locales/").stdout
    return sorted(name for name in output.splitlines() if CATALOG_RE.match(name))


def _current_catalog_names(repo_root: Path) -> list[str]:
    directory = repo_root / "locales"
    if not directory.is_dir():
        return []
    names = []
    for entry in directory.iterdir():
        if not entry.is_file():
            continue
        name = f"locales/{entry.name}"
        if CATALOG_RE.match(name):
            names.append(name)
    return sorted(names)


def classify_repo(repo_root: Path, base: str, changed: Iterable[str]) -> str:
    """Classify the working tree against ``base``; fail closed on any error."""

    changed = list(changed)
    if not changed:
        return VALUE_AFFECTING
    if any(not CATALOG_RE.match(name) for name in changed):
        return VALUE_AFFECTING
    try:
        base_names = _base_catalog_names(repo_root, base)
        current_names = _current_catalog_names(repo_root)
        if base_names != current_names:
            return VALUE_AFFECTING
        base_texts: dict[str, str] = {}
        current_texts: dict[str, str] = {}
        for name in base_names:
            base_texts[name] = _git(repo_root, "show", f"{base}:{name}").stdout
            path = repo_root / name
            if not path.is_file():
                return VALUE_AFFECTING
            current_texts[name] = path.read_text(encoding="utf-8")
    except (OSError, subprocess.SubprocessError, UnicodeDecodeError):
        return VALUE_AFFECTING
    return classify_text(base_texts, current_texts, changed)


def _json(mapping: Mapping[str, str]) -> str:
    return json.dumps(mapping)


def _self_test_cases() -> list[tuple[str, str, dict[str, str], dict[str, str], list[str]]]:
    en = {"tab.performance": "Performance", "tooltip.cancel": "Cancel"}
    zh = {"tab.performance": "性能", "tooltip.cancel": "取消"}
    base = {"locales/en.json": _json(en), "locales/zh.json": _json(zh)}
    both = ["locales/en.json", "locales/zh.json"]
    cases: list[tuple[str, str, dict[str, str], dict[str, str], list[str]]] = []

    def add(label: str, expected: str, current: dict[str, str], changed: list[str] | None = None) -> None:
        cases.append((label, expected, base, current, changed or both))

    # The two safe shapes: whole keys added / removed in every catalog.
    add(
        "pure-key-addition",
        KEY_SET_ONLY,
        {
            "locales/en.json": _json({**en, "tab.alerts": "Alerts"}),
            "locales/zh.json": _json({**zh, "tab.alerts": "警报"}),
        },
    )
    add(
        "pure-key-removal",
        KEY_SET_ONLY,
        {
            "locales/en.json": _json({"tab.performance": "Performance"}),
            "locales/zh.json": _json({"tab.performance": "性能"}),
        },
    )
    # A surviving translated value changed.
    add(
        "value-change",
        VALUE_AFFECTING,
        {
            "locales/en.json": _json({"tab.performance": "Perf", "tooltip.cancel": "Cancel"}),
            "locales/zh.json": _json(zh),
        },
    )
    # A value changed on a key that is also removed from the other catalog.
    add(
        "value-change-on-removed-key",
        VALUE_AFFECTING,
        {
            "locales/en.json": _json({"tab.performance": "Perf"}),
            "locales/zh.json": _json({"tooltip.cancel": "取消"}),
        },
    )
    # Unparsable / wrong-shape catalogs.
    add("unparsable", VALUE_AFFECTING, {"locales/en.json": "{ not json", "locales/zh.json": _json(zh)})
    add("json-array", VALUE_AFFECTING, {"locales/en.json": '["a"]', "locales/zh.json": _json(zh)})
    add(
        "nested-value",
        VALUE_AFFECTING,
        {"locales/en.json": _json({"tab.performance": {"x": "y"}}), "locales/zh.json": _json(zh)},
    )
    add("duplicate-key", VALUE_AFFECTING, {"locales/en.json": '{"a": "1", "a": "2"}', "locales/zh.json": _json(zh)})
    # Symmetry / shape moved.
    add(
        "asymmetric-addition",
        VALUE_AFFECTING,
        {
            "locales/en.json": _json({**en, "tab.alerts": "Alerts"}),
            "locales/zh.json": _json(zh),
        },
    )
    add(
        "asymmetric-removal",
        VALUE_AFFECTING,
        {
            "locales/en.json": _json({"tab.performance": "Performance"}),
            "locales/zh.json": _json(zh),
        },
    )
    # Catalog file set changed (a new or removed locale).
    add(
        "catalog-set-changed",
        VALUE_AFFECTING,
        {
            "locales/en.json": _json(en),
            "locales/zh.json": _json(zh),
            "locales/fr.json": _json(en),
        },
    )
    # A missing side and the empty change are ambiguous.
    cases.append(
        (
            "missing-side",
            VALUE_AFFECTING,
            base,
            {"locales/zh.json": _json(zh)},
            both,
        )
    )
    cases.append(("empty-change", VALUE_AFFECTING, base, base, []))

    # A pre-existing asymmetric fixture must survive an unrelated symmetric edit.
    en_fallback = {**en, "fallback.sample": "Sample"}
    base_fallback = {"locales/en.json": _json(en_fallback), "locales/zh.json": _json(zh)}
    cases.append(
        (
            "fallback-fixture-tolerated",
            KEY_SET_ONLY,
            base_fallback,
            {
                "locales/en.json": _json({**en_fallback, "tab.alerts": "Alerts"}),
                "locales/zh.json": _json({**zh, "tab.alerts": "警报"}),
            },
            both,
        )
    )
    cases.append(
        (
            "fallback-fixture-removed",
            VALUE_AFFECTING,
            base_fallback,
            {"locales/en.json": _json(en), "locales/zh.json": _json(zh)},
            both,
        )
    )
    return cases


def self_test() -> int:
    failures: list[str] = []
    for label, expected, base, current, changed in _self_test_cases():
        observed = classify_text(base, current, changed)
        if observed != expected:
            failures.append(f"{label}: expected {expected}, observed {observed}")

    # Exercise the git/IO read path once, including a repo-level unparsable file.
    with tempfile.TemporaryDirectory(prefix="locale-classifier-") as directory:
        root = Path(directory)
        root.joinpath("locales").mkdir()
        identity = {
            "GIT_AUTHOR_NAME": "locale-keyset-classifier",
            "GIT_AUTHOR_EMAIL": "locale-keyset-classifier@users.noreply.github.com",
            "GIT_COMMITTER_NAME": "locale-keyset-classifier",
            "GIT_COMMITTER_EMAIL": "locale-keyset-classifier@users.noreply.github.com",
        }
        backup = {key: os.environ.get(key) for key in identity}
        os.environ.update(identity)
        try:
            _git(root, "init", "-q")
            root.joinpath("locales", "en.json").write_text(_json({"a": "A"}), encoding="utf-8")
            root.joinpath("locales", "zh.json").write_text(_json({"a": "甲"}), encoding="utf-8")
            _git(root, "add", "-A")
            _git(root, "commit", "-q", "-m", "baseline")
            base = _git(root, "rev-parse", "HEAD").stdout.strip()
            root.joinpath("locales", "en.json").write_text(_json({"a": "A", "b": "B"}), encoding="utf-8")
            root.joinpath("locales", "zh.json").write_text(_json({"a": "甲", "b": "乙"}), encoding="utf-8")
            observed = classify_repo(root, base, ["locales/en.json", "locales/zh.json"])
            if observed != KEY_SET_ONLY:
                failures.append(f"repo-key-addition: expected {KEY_SET_ONLY}, observed {observed}")
            root.joinpath("locales", "en.json").write_text("{ not json", encoding="utf-8")
            observed = classify_repo(root, base, ["locales/en.json"])
            if observed != VALUE_AFFECTING:
                failures.append(f"repo-unparsable: expected {VALUE_AFFECTING}, observed {observed}")
        except (OSError, subprocess.SubprocessError) as exc:
            failures.append(f"repo-path setup failed: {exc}")
        finally:
            for key, value in backup.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    if failures:
        for failure in failures:
            print(f"locale-keyset-classifier: self-test FAIL: {failure}", file=sys.stderr)
        return 1
    print(f"locale-keyset-classifier: self-test PASS ({len(_self_test_cases()) + 2} cases)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--base", default="HEAD")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("changed", nargs="*", help="changed paths under locales/")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    print(classify_repo(args.repo_root.resolve(), args.base, args.changed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
