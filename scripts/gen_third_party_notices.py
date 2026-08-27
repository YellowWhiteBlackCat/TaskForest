#!/usr/bin/env python3
"""Generate the THIRD-PARTY-NOTICES.txt shipped inside every binary package.

Scope: every third-party crate that `cargo metadata --locked` resolves as a
non-development dependency (normal or build edge) of any TaskForest workspace
member, across every supported target and UI shape. Workspace members are
TaskForest's own crates and stay under the repository LICENSE. Dev-only
dependencies (test and example tooling) never enter a shipped artifact and are
out of scope by design — docs/ACKNOWLEDGMENTS.md names them separately.

For each crate the generator copies the license text files found in the crate's
source distribution (LICENSE*, COPYING*, COPYRIGHT*, NOTICE*), deduplicated by
content, and appends the OFL terms of the fonts embedded in the release
binaries. Output is deterministic: sorted crates, no timestamps, LF endings.

Fail-closed: a crate with neither a license expression nor any license file
aborts the run; a missing text with a valid expression is reported on stderr
and still recorded in the table.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Dev edges never ship; build edges are included so proc-macro and build
# tooling that participates in producing the artifact stays credited.
# cargo metadata reports normal dependencies with kind null.
PRODUCTION_KINDS = frozenset({"normal", "build"})

LICENSE_FILE_RE = re.compile(
    r"^(un)?licen[cs]e|^(copying|copyright|notice)\b", re.IGNORECASE
)

FONT_APPENDIX = (
    ("MiSans VF — bundled font (SIL OFL 1.1)",
     "crates/taskmanager-assets/assets/fonts/OFL-1.1.txt"),
    ("Roboto Mono VF — bundled font (SIL OFL 1.1)",
     "crates/taskmanager-assets/assets/fonts/OFL-RobotoMono.txt"),
    ("Bundled font provenance and terms",
     "crates/taskmanager-assets/assets/fonts/LICENSE.md"),
)

METADATA_TIMEOUT_S = 120


def run_cargo_metadata() -> dict:
    proc = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=METADATA_TIMEOUT_S,
    )
    if proc.returncode != 0:
        raise ValueError(f"cargo metadata failed: {proc.stderr.strip()}")
    return json.loads(proc.stdout)


def production_closure(metadata: dict) -> list[dict]:
    """Third-party packages reachable over non-dev edges from the workspace."""
    members = set(metadata["workspace_members"])
    edges: dict[str, list[str]] = {}
    for node in metadata["resolve"]["nodes"]:
        targets = [
            dep["pkg"]
            for dep in node["deps"]
            if any((kind.get("kind") or "normal") in PRODUCTION_KINDS
                   for kind in dep.get("dep_kinds", []))
        ]
        edges[node["id"]] = targets
    # Bounded reachability: each round absorbs the current frontier, and the
    # closure converges within one round per node in the graph, so the loop
    # is bounded by the resolve-node count rather than an open while.
    seen: set[str] = set()
    frontier: set[str] = set(members)
    for _ in range(len(metadata["resolve"]["nodes"]) + 1):
        if not frontier:
            break
        seen |= frontier
        frontier = {succ for node in frontier
                    for succ in edges.get(node, ())} - seen
    packages = [
        package for package in metadata["packages"]
        if package["id"] in seen and package["id"] not in members
    ]
    return sorted(packages, key=lambda package: (package["name"], package["version"]))


def find_license_files(package: dict) -> list[Path]:
    """License-bearing files of a package, `license_file` first if declared."""
    root = Path(package["manifest_path"]).parent
    if not root.is_dir():
        return []
    declared = package.get("license_file")
    if declared:
        candidate = root / declared
        if candidate.is_file():
            return [candidate]
    return sorted(
        entry for entry in root.iterdir()
        if entry.is_file() and LICENSE_FILE_RE.match(entry.name)
    )


def read_text(path: Path) -> str | None:
    """Decode a license file; None for binary payloads."""
    data = path.read_bytes()
    if b"\x00" in data:
        return None
    for encoding in ("utf-8", "utf-8-sig", "cp1252"):
        try:
            return data.decode(encoding)
        except UnicodeDecodeError:
            continue
    return data.decode("latin-1")


def normalize(text: str) -> str:
    return "\n".join(line.rstrip() for line in text.replace("\r\n", "\n").split("\n")).strip()


def collect_sections(packages: list[dict]) -> tuple[list[str], dict[str, list[str]], list[dict]]:
    """Deduplicated license texts, per-crate section refs, offending crates.

    Returns (sections, refs-by-package-id, offenders) where each section is the
    normalized text and refs map package ids to "[Tn]" markers; a crate whose
    text is identical to an earlier crate's shares its marker.
    """
    sections: list[str] = []
    by_hash: dict[str, int] = {}
    refs: dict[str, list[str]] = {}
    offenders: list[dict] = []
    for package in packages:
        package_refs: list[str] = []
        for path in find_license_files(package):
            text = read_text(path)
            if text is None:
                continue
            digest = hashlib.sha256(normalize(text).encode("utf-8")).hexdigest()
            index = by_hash.get(digest)
            if index is None:
                index = len(sections)
                by_hash[digest] = index
                sections.append(normalize(text))
            package_refs.append(f"[T{index + 1}]")
        refs[package["id"]] = sorted(set(package_refs))
        if not package.get("license") and not package_refs:
            offenders.append(package)
    return sections, refs, offenders


def build_document(packages: list[dict], sections: list[str],
                   refs: dict[str, list[str]], missing_text: list[dict]) -> str:
    lines: list[str] = [
        "TaskForest third-party notices",
        "==============================",
        "",
        "This file is generated at package build time by",
        "scripts/gen_third_party_notices.py from the Cargo.lock dependency",
        "graph. It lists every third-party crate resolved as a",
        "non-development dependency of any TaskForest workspace member across",
        "every supported target and UI shape, together with the license texts",
        "found in each crate's source distribution. Versions are pinned by",
        "Cargo.lock, the sole version authority. TaskForest's own workspace",
        "crates are licensed under Apache-2.0 (see the accompanying LICENSE).",
        "",
        f"{len(packages)} third-party crates, {len(sections)} unique license texts.",
        "",
        "Dependency inventory",
        "--------------------",
        "",
    ]
    for package in packages:
        expression = package.get("license") or "(no machine-readable expression)"
        repository = package.get("repository") or package.get("homepage") or ""
        lines.append(
            f"{package['name']} {package['version']} — {expression}"
            + (f" — {repository}" if repository else "")
        )
        markers = refs.get(package["id"]) or []
        if markers:
            lines.append(f"    license texts: {' '.join(markers)}")
        elif package.get("license"):
            lines.append("    license texts: none found in the crate source;"
                         " expression recorded above")
    lines += [
        "",
        "License texts",
        "-------------",
        "",
    ]
    for index, section in enumerate(sections, start=1):
        lines.append(f"[T{index}]")
        lines.append(section)
        lines.append("")
    lines += [
        "Bundled fonts",
        "-------------",
        "",
        "The release binaries embed the fonts below; their full terms ship in",
        "this notice so a binary-only distribution carries them.",
        "",
    ]
    for title, relative in FONT_APPENDIX:
        path = REPO_ROOT / relative
        text = normalize(read_text(path) or "")
        if not text:
            raise ValueError(f"font license file missing or empty: {relative}")
        lines.append(f"== {title} ({relative}) ==")
        lines.append(text)
        lines.append("")
    if missing_text:
        lines += [
            "Crates without a license text file in their source distribution",
            "---------------------------------------------------------------",
            "",
            "These crates declare a machine-readable license expression",
            "(recorded in the inventory above) but ship no standalone license",
            "text file; the expression and the crate's source remain their",
            "licensing record:",
            "",
        ]
        lines.extend(f"- {package['name']} {package['version']}" for package in missing_text)
        lines.append("")
    return "\n".join(lines).rstrip("\n") + "\n"


def generate(output: Path) -> tuple[int, int]:
    metadata = run_cargo_metadata()
    packages = production_closure(metadata)
    if not packages:
        raise ValueError("production closure is empty; refusing to emit a vacuous notice")
    sections, refs, offenders = collect_sections(packages)
    if offenders:
        names = ", ".join(f"{package['name']} {package['version']}" for package in offenders)
        raise ValueError(f"no license expression and no license file: {names}")
    missing_text = [
        package for package in packages
        if package.get("license") and not refs.get(package["id"])
    ]
    if missing_text:
        print(
            "gen-third-party-notices: WARNING no license text file for "
            + ", ".join(f"{package['name']} {package['version']}" for package in missing_text),
            file=sys.stderr,
        )
    document = build_document(packages, sections, refs, missing_text)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(document, encoding="utf-8", newline="\n")
    return len(packages), len(sections)


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        mit = root / "mit"
        mit.mkdir()
        (mit / "LICENSE-MIT").write_text("MIT text\r\n", encoding="utf-8")
        (mit / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
        assert [p.name for p in find_license_files(
            {"manifest_path": str(mit / "Cargo.toml"), "license_file": None})] == ["LICENSE-MIT"]
        copying = root / "copying"
        copying.mkdir()
        (copying / "COPYING.txt").write_text("copying text", encoding="utf-8")
        (copying / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
        assert [p.name for p in find_license_files(
            {"manifest_path": str(copying / "Cargo.toml"), "license_file": None})] == ["COPYING.txt"]
        declared = root / "declared"
        declared.mkdir()
        (declared / "LICENSE-NAMED").write_text("named", encoding="utf-8")
        (declared / "NOTICE").write_text("notice", encoding="utf-8")
        (declared / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
        assert [p.name for p in find_license_files(
            {"manifest_path": str(declared / "Cargo.toml"),
             "license_file": "LICENSE-NAMED"})] == ["LICENSE-NAMED"]
        binary = root / "binary"
        binary.mkdir()
        (binary / "LICENSE").write_bytes(b"l\x00cense")
        assert read_text(binary / "LICENSE") is None
        sections, refs, offenders = collect_sections([
            {"id": "x", "license": "MIT", "manifest_path": str(mit / "Cargo.toml"), "license_file": None},
            {"id": "y", "license": "MIT", "manifest_path": str(mit / "Cargo.toml"), "license_file": None},
            {"id": "z", "license": None, "manifest_path": str(root / "missing" / "Cargo.toml"), "license_file": None},
        ])
        assert len(sections) == 1 and refs["x"] == refs["y"] == ["[T1]"]
        assert [package["id"] for package in offenders] == ["z"]
    metadata = {
        "workspace_members": ["member"],
        "packages": [
            {"id": "member", "name": "member", "version": "0.0.0"},
            {"id": "prod", "name": "prod", "version": "1.0"},
            {"id": "build-tool", "name": "build-tool", "version": "1.0"},
            {"id": "dev-only", "name": "dev-only", "version": "1.0"},
        ],
        "resolve": {"nodes": [
            {"id": "member", "deps": [
                {"pkg": "prod", "dep_kinds": [{"kind": None, "target": None}]},
                {"pkg": "dev-only", "dep_kinds": [{"kind": "dev", "target": None}]},
            ]},
            {"id": "prod", "deps": [
                {"pkg": "build-tool", "dep_kinds": [{"kind": "build", "target": None}]},
            ]},
            {"id": "build-tool", "deps": []},
            {"id": "dev-only", "deps": []},
        ]},
    }
    closure = production_closure(metadata)
    assert [package["id"] for package in closure] == ["build-tool", "prod"]
    assert normalize("a \r\nb \n") == "a\nb"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", nargs="?", type=Path,
                        help="destination file, e.g. target/release/THIRD-PARTY-NOTICES.txt")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Third-party notices generator self-test: PASS")
        return 0
    if args.output is None:
        parser.error("output is required unless --self-test is given")
    try:
        crates, texts = generate(args.output)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"gen-third-party-notices: ERROR {error}", file=sys.stderr)
        return 1
    print(f"gen-third-party-notices: {crates} crates, {texts} license texts"
          f" -> {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
