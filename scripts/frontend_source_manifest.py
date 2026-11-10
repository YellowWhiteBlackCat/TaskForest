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
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

FRONTEND_PACKAGES = {
    "gpui": "taskmanager-gpui",
    "tui": "taskmanager-tui",
    "iced": "taskmanager-iced",
    "bevy": "taskmanager-bevy-ui",
}
SHARED_REGISTRY_PACKAGES = frozenset(
    {
        "zhuge-ui-core",
        "zhuge-ui-tokens",
        "zhuge-ui-kit-iced",
        "zhuge-ui-kit-bevy",
    }
)
FRONTEND_SCRIPTS = {
    "gpui": (
        "scripts/capture-windows.sh",
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
        "scripts/capture_tui_scenarios.tsv",
        "scripts/capture_tui_terminal_env.sh",
        "scripts/capture_tui_terminal_profiles.py",
        "scripts/capture_tui_terminal_profiles.tsv",
        "scripts/private-session.conf",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/validate_tui_evidence.py",
        "scripts/validate_tui_health_review_events.py",
        "scripts/validate_tui_apps_empty.py",
        "scripts/validate_capture_evidence.py",
    ),
    "iced": (
        "scripts/capture-iced-windows.sh",
        "scripts/capture-iced.sh",
        "scripts/capture-iced-matrix.sh",
        "scripts/private-session.conf",
        "scripts/capture_supervisor.py",
        "scripts/capture_build.py",
        "scripts/capture_publish.py",
        "scripts/capture-reclaim.sh",
        "scripts/capture_iced_scenarios.tsv",
        "scripts/validate_iced_matrix.py",
        "scripts/validate_iced_windows_capture.py",
        "scripts/validate_capture_evidence.py",
        "scripts/validate_tui_evidence.py",
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
        "scripts/validate_capture_evidence.py",
        "scripts/validate_tui_evidence.py",
        "scripts/accept-bevy-interactions.sh",
    ),
}
ROOT_FILES = (
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "locales/en.json",
    "locales/zh.json",
    "scripts/capture_history_fixtures.py",
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
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=root,
        check=True,
        text=True,
        capture_output=True,
        timeout=60,
    )
    metadata = json.loads(output.stdout)
    packages: dict[str, dict] = {}
    registry_ids: dict[str, str] = {}
    for package in metadata["packages"]:
        if package["name"] not in SHARED_REGISTRY_PACKAGES:
            if package.get("source") is not None:
                continue
            packages[package["name"]] = package
            continue
        source = package.get("source") or ""
        if not source.startswith("registry+"):
            raise ValueError(f"shared package {package['name']} is not resolved from crates.io: {source!r}")
        previous_id = registry_ids.get(package["name"])
        if previous_id and previous_id != package["id"]:
            raise ValueError(f"ambiguous shared registry release: {package['name']}")
        registry_ids[package["name"]] = package["id"]
        previous = packages.get(package["name"])
        if previous and previous["manifest_path"] != package["manifest_path"]:
            raise ValueError(f"ambiguous package owner: {package['name']}")
        packages[package["name"]] = package
    pending = list(packages.values())
    seen_manifests = {Path(package["manifest_path"]).resolve() for package in pending}
    cursor = 0
    while cursor < len(pending):
        package = pending[cursor]
        cursor += 1
        for dependency in package["dependencies"]:
            if dependency.get("kind") not in (None, "normal") or not dependency.get("path"):
                continue
            manifest = (Path(dependency["path"]) / "Cargo.toml").resolve()
            if manifest in seen_manifests:
                continue
            output = subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked", "--manifest-path", str(manifest)],
                cwd=root, check=True, text=True, capture_output=True, timeout=60,
            )
            for local in json.loads(output.stdout)["packages"]:
                local_manifest = Path(local["manifest_path"]).resolve()
                existing = packages.get(local["name"])
                if existing and Path(existing["manifest_path"]).resolve() != local_manifest:
                    raise ValueError(f"ambiguous local package owner: {local['name']}")
                if local_manifest not in seen_manifests:
                    seen_manifests.add(local_manifest)
                    packages[local["name"]] = local
                    pending.append(local)
    return packages


def registry_release_paths(root: Path, packages: dict[str, dict]) -> list[Path]:
    """Bind a shared registry dependency by its exact Cargo.lock release entry."""

    lock_path = root / "Cargo.lock"
    lock_text = lock_path.read_text(encoding="utf-8")
    paths: set[Path] = set()
    for name in SHARED_REGISTRY_PACKAGES:
        package = packages.get(name)
        if package is None:
            continue
        source = package["source"]
        if not source.startswith("registry+"):
            raise ValueError(f"shared package {name} is not resolved from crates.io")
        needle = (
            f'name = "{name}"\n'
            f'version = "{package["version"]}"\n'
            f'source = "{source}"\n'
        )
        if needle not in lock_text:
            raise ValueError(f"Cargo.lock does not pin the resolved {name} registry release")
        paths.add(lock_path)
    return sorted(paths)


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
    paths.update(registry_release_paths(root, packages))
    for relative in ROOT_FILES:
        path = root / relative
        if path.is_file():
            paths.add(path)
    for package_name in package_scope(root, packages, frontend):
        if package_name in SHARED_REGISTRY_PACKAGES:
            continue
        manifest = Path(packages[package_name]["manifest_path"]).resolve()
        paths.add(manifest)
        package_root = manifest.parent
        if not package_root.is_relative_to(root):
            for parent in package_root.parents:
                workspace_manifest = parent / "Cargo.toml"
                if workspace_manifest.is_file():
                    paths.add(workspace_manifest)
                    for name in ("rust-toolchain.toml", ".cargo/config.toml"):
                        path = parent / name
                        if path.is_file():
                            paths.add(path)
                    break
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
    return sorted(paths, key=lambda path: source_name(root, path))


def source_name(root: Path, path: Path) -> str:
    """Portable labels also cover Cargo-declared sibling path dependencies."""
    return Path(os.path.relpath(path, root)).as_posix()


def write_manifest(root: Path, frontend: str, output: Path) -> int:
    paths = source_paths(root, frontend)
    lines = [f"{digest(path)}  {source_name(root, path)}" for path in paths]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"frontend source scope: {frontend} ({len(lines)} files)")
    return 0


def self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        source = root / "source.rs"
        source.write_text("pub fn current() {}\n", encoding="utf-8")
        output = root / "clean-target" / "frontend-source-manifests" / "gpui.txt"
        with patch(__name__ + ".source_paths", return_value=[source]):
            assert write_manifest(root, "gpui", output) == 0
            assert output.read_text(encoding="utf-8") == f"{digest(source)}  source.rs\n"
        sibling = root / "libs" / "shared" / "src.rs"
        sibling.parent.mkdir(parents=True)
        sibling.write_text("pub fn shared() {}\n", encoding="utf-8")
        product = root / "product"
        product.mkdir()
        with patch(__name__ + ".source_paths", return_value=[sibling]):
            assert write_manifest(product, "iced", output) == 0
            original = output.read_text(encoding="utf-8")
            assert original == f"{digest(sibling)}  ../libs/shared/src.rs\n"
            sibling.write_text("pub fn changed() {}\n", encoding="utf-8")
            assert write_manifest(product, "iced", output) == 0
            assert original != output.read_text(encoding="utf-8")
        registry_root = root / "registry"
        registry_root.mkdir()
        lock = registry_root / "Cargo.lock"
        metadata = {"packages": [
            {
                "name": name,
                "version": "1.0.0",
                "source": "registry+https://github.com/rust-lang/crates.io-index",
                "manifest_path": str(registry_root / name / "Cargo.toml"),
                "id": f"registry+https://github.com/rust-lang/crates.io-index#{name}@1.0.0",
                "dependencies": [],
            }
            for name in SHARED_REGISTRY_PACKAGES
        ]}
        lock.write_text(
            "".join(
                f'[[package]]\nname = "{name}"\nversion = "1.0.0"\n'
                f'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
                f'checksum = "{index * 64:064x}"\n'
                for index, name in enumerate(SHARED_REGISTRY_PACKAGES, 1)
            ),
            encoding="utf-8",
        )
        with patch(
            "subprocess.run",
            return_value=subprocess.CompletedProcess([], 0, json.dumps(metadata), ""),
        ):
            packages = workspace_packages(registry_root)
            assert set(SHARED_REGISTRY_PACKAGES) <= set(packages)
            assert registry_release_paths(registry_root, packages) == [lock]
            assert source_paths(registry_root, "bevy") == [lock]
        lock.write_text(lock.read_text(encoding="utf-8").replace('version = "1.0.0"', 'version = "1.0.1"', 1), encoding="utf-8")
        try:
            registry_release_paths(registry_root, packages)
        except ValueError:
            pass
        else:
            raise AssertionError("a stale Cargo.lock release must fail closed")
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
