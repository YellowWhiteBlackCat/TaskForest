#!/usr/bin/env python3
"""Write a frontend-scoped source provenance manifest.

Evidence must expire when the selected frontend or one of its shared workspace
dependencies changes. It must not expire because an unrelated frontend changed.
The workspace dependency graph is read from Cargo metadata so the scope follows
real package ownership instead of a broad ``find crates src`` approximation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

FRONTEND_PACKAGES = {
    "gpui": "taskmanager-gpui",
    "tui": "taskmanager-tui",
    "iced": "taskmanager-iced",
    "bevy": "taskmanager-bevy-ui",
}
FRONTEND_SCRIPTS = {
    "gpui": (
        "scripts/capture-niri.sh",
        "scripts/capture-host-wayland-diagnostic.sh",
        "scripts/capture_scenarios.tsv",
        "scripts/private-session.conf",
        "scripts/receive_kwin_window_receipt.py",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/test_capture_isolation.py",
        "scripts/validate_capture_evidence.py",
        "scripts/validate_host_wayland_diagnostic.py",
    ),
    "tui": (
        "scripts/capture-tui.sh",
        "scripts/private-session.conf",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/validate_tui_evidence.py",
    ),
    "iced": (
        "scripts/capture-iced.sh",
        "scripts/capture-iced-matrix.sh",
        "scripts/private-session.conf",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/capture_iced_scenarios.tsv",
        "scripts/validate_iced_matrix.py",
    ),
    "bevy": (
        "scripts/capture-bevy.sh",
        "scripts/private-session.conf",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/capture_bevy_scenarios.tsv",
        "scripts/validate_bevy_matrix.py",
        "scripts/accept-bevy-interactions.sh",
        "scripts/bevy_interaction_matrix.tsv",
    ),
}
ROOT_FILES = (
    "locales/en.json",
    "locales/zh.json",
)


def host_target_os() -> str:
    """Return Cargo's target_os value for the current capture host."""

    if sys.platform.startswith("linux"):
        return "linux"
    if sys.platform == "darwin":
        return "macos"
    if sys.platform in {"win32", "cygwin", "msys"}:
        return "windows"
    return sys.platform


def dependency_applies_to_host(dependency: dict) -> bool:
    """Keep only target dependencies compiled for the capture host.

    Cargo metadata exposes target-specific dependencies as a normalized cfg
    string.  Unknown target expressions are retained conservatively; the
    important matrix rule is that a Linux receipt must not include the
    macOS/Windows adapters hidden behind `taskmanager-platform-native`.
    """

    target = dependency.get("target")
    if target is None:
        return True
    for operating_system in ("linux", "macos", "windows"):
        marker = f'target_os = "{operating_system}"'
        if marker in target:
            return operating_system == host_target_os()
    return True


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def workspace_packages(root: Path) -> dict[str, dict]:
    output = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=root,
        check=True,
        text=True,
        capture_output=True,
        timeout=60,
    )
    metadata = json.loads(output.stdout)
    return {package["name"]: package for package in metadata["packages"]}


def package_scope(root: Path, packages: dict[str, dict], frontend: str) -> set[str]:
    # ADR-051: each frontend is its own product crate. Provenance follows the
    # production graph walked from the product crate — no feature simulation
    # and no root dispatch package exist anymore.
    selected = FRONTEND_PACKAGES[frontend]
    roots = {selected}
    included: set[str] = set()
    pending = list(roots)
    cursor = 0
    while cursor < len(pending):
        name = pending[cursor]
        cursor += 1
        if name in included or name not in packages:
            continue
        included.add(name)
        package = packages[name]
        for dependency in package["dependencies"]:
            # Provenance follows the production graph. Dev-only helpers are
            # not linked into a release binary and must not invalidate pixel
            # receipts.
            if dependency.get("kind") not in (None, "normal"):
                continue
            if not dependency_applies_to_host(dependency):
                continue
            dep_name = dependency["name"]
            if dep_name in packages:
                pending.append(dep_name)
    return included


def source_paths(root: Path, frontend: str) -> list[Path]:
    packages = workspace_packages(root)
    paths: set[Path] = set()
    for relative in ROOT_FILES:
        path = root / relative
        if path.is_file():
            paths.add(path)
    for package_name in package_scope(root, packages, frontend):
        manifest = Path(packages[package_name]["manifest_path"]).resolve()
        paths.add(manifest)
        package_root = manifest.parent
        # Every walked package is a workspace member whose `src` tree affects
        # the selected product's receipt.
        source_root = package_root / "src"
        if source_root.is_dir():
            paths.update(path for path in source_root.rglob("*") if path.is_file())
        build_script = package_root / "build.rs"
        if build_script.is_file():
            paths.add(build_script)
    patch_root = root / "patches" / "proc-macro-error2"
    if patch_root.is_dir():
        paths.update(path for path in patch_root.rglob("*") if path.is_file())
    for relative in FRONTEND_SCRIPTS[frontend]:
        path = root / relative
        if path.is_file():
            paths.add(path)
    generator = root / "scripts" / "frontend_source_manifest.py"
    if generator.is_file():
        paths.add(generator)
    return sorted(paths, key=lambda path: path.relative_to(root).as_posix())


def write_manifest(root: Path, frontend: str, output: Path) -> int:
    paths = source_paths(root, frontend)
    lines = [f"{digest(path)}  {path.relative_to(root).as_posix()}" for path in paths]
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"frontend source scope: {frontend} ({len(lines)} files)")
    return 0


def self_test() -> int:
    assert FRONTEND_PACKAGES["gpui"] == "taskmanager-gpui"
    assert FRONTEND_PACKAGES["tui"] != FRONTEND_PACKAGES["iced"]
    assert FRONTEND_PACKAGES["bevy"] == "taskmanager-bevy-ui"
    assert "src/gpui_app" not in " ".join(FRONTEND_SCRIPTS["tui"])
    assert "scripts/capture-tui.sh" in FRONTEND_SCRIPTS["tui"]
    assert "scripts/capture-niri.sh" not in FRONTEND_SCRIPTS["tui"]
    assert "scripts/capture-bevy.sh" in FRONTEND_SCRIPTS["bevy"]
    assert dependency_applies_to_host({"target": None})
    assert dependency_applies_to_host(
        {"target": f'cfg(target_os = "{host_target_os()}")'}
    )
    other_os = next(
        operating_system
        for operating_system in ("linux", "macos", "windows")
        if operating_system != host_target_os()
    )
    assert not dependency_applies_to_host(
        {"target": f'cfg(target_os = "{other_os}")'}
    )
    print("frontend source manifest self-test: PASS")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frontend", choices=tuple(FRONTEND_PACKAGES))
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.frontend is None or args.output is None:
        parser.error("--frontend and --output are required unless --self-test is used")
    try:
        return write_manifest(args.repo_root.resolve(), args.frontend, args.output)
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired, ValueError, json.JSONDecodeError) as error:
        print(f"frontend source manifest: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
