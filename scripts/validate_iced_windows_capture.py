#!/usr/bin/env python3
"""Fail-closed validator for a paired Windows Iced/GPUI capture bundle."""

from __future__ import annotations

import argparse
import hashlib
import re
import struct
import subprocess
import sys
import tomllib
import zlib
from pathlib import Path, PureWindowsPath


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
CAPTURE_API = "windows.graphics.capture (windows-capture 1.5.0)"
PAIRED_REQUESTED_SIZE = "1180x780"
ICED_REQUIRED = {
    "frontend",
    "target",
    "scenario",
    "pair_id",
    "theme",
    "high_contrast",
    "requested_window_size",
    "window_size",
    "window_rect_size",
    "capture_api",
    "window_title",
    "window_process_name",
    "target_pid",
    "binary",
    "binary_sha256",
    "git_head",
    "worktree",
    "rust",
    "cargo_lock_sha256",
    "renderer_stack",
    "renderer_active",
    "text_stack",
    "text_shaping",
    "text_rasterizer",
    "text_rasterizer_active",
    "native_text_api",
    "font_weight_axis_policy",
    "font_weight_450_name",
    "font_weight_450_value",
    "source_scope",
    "source_manifest",
    "source_manifest_sha256",
    "marker_file",
    "marker_sha256",
    "target_metadata",
    "target_metadata_sha256",
    "peer_receipt",
    "peer_capture_metadata",
    "peer_image",
    "peer_source_manifest",
    "peer_frontend",
    "peer_target",
    "peer_scenario",
    "peer_theme",
    "peer_high_contrast",
    "peer_pair_id",
    "peer_requested_window_size",
    "peer_window_size",
    "peer_png_sha256",
    "peer_binary_sha256",
    "peer_source_manifest_sha256",
    "timestamp_utc",
    "png_sha256",
}
GPUI_REQUIRED = {
    "frontend",
    "target",
    "scenario",
    "pair_id",
    "theme",
    "high_contrast",
    "requested_window_size",
    "window_size",
    "git_head",
    "worktree",
    "rust",
    "binary",
    "binary_sha256",
    "source_manifest_sha256",
}


class EvidenceError(RuntimeError):
    """Raised when evidence is incomplete or inconsistent."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_metadata(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise EvidenceError(f"cannot read metadata {path}: {error}") from error
    for line in lines:
        key, separator, value = line.partition("=")
        if not separator or not key or not value or key in values:
            raise EvidenceError(f"invalid or duplicate metadata line: {line!r}")
        values[key] = value
    return values


def positive_size(value: str, name: str) -> tuple[int, int]:
    match = re.fullmatch(r"([0-9]+)x([0-9]+)", value)
    if match is None:
        raise EvidenceError(f"{name} is not WIDTHxHEIGHT: {value!r}")
    width, height = (int(part) for part in match.groups())
    if width < 1 or height < 1:
        raise EvidenceError(f"{name} must be positive: {value!r}")
    return width, height


def png_dimensions(path: Path) -> tuple[int, int]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise EvidenceError(f"cannot read PNG {path}: {error}") from error
    if len(data) < 33 or not data.startswith(PNG_SIGNATURE):
        raise EvidenceError(f"invalid PNG signature or truncated PNG: {path}")

    offset = len(PNG_SIGNATURE)
    width = height = 0
    saw_ihdr = saw_idat = saw_iend = False
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        end = offset + 8 + length
        crc_end = end + 4
        if crc_end > len(data):
            raise EvidenceError(f"truncated PNG chunk: {path}")
        chunk = data[offset + 8 : end]
        expected_crc = struct.unpack(">I", data[end:crc_end])[0]
        actual_crc = zlib.crc32(kind)
        actual_crc = zlib.crc32(chunk, actual_crc) & 0xFFFFFFFF
        if expected_crc != actual_crc:
            raise EvidenceError(f"PNG CRC mismatch in {kind!r}: {path}")
        if not saw_ihdr:
            if kind != b"IHDR" or length != 13:
                raise EvidenceError(f"PNG does not start with a valid IHDR: {path}")
            width, height = struct.unpack(">II", chunk[:8])
            if width < 1 or height < 1:
                raise EvidenceError(f"PNG has empty dimensions: {path}")
            saw_ihdr = True
        elif kind == b"IHDR":
            raise EvidenceError(f"PNG contains duplicate IHDR: {path}")
        if kind == b"IDAT":
            saw_idat = True
        if kind == b"IEND":
            if length != 0:
                raise EvidenceError(f"PNG has an invalid IEND: {path}")
            saw_iend = True
            offset = crc_end
            break
        offset = crc_end

    if not (saw_ihdr and saw_idat and saw_iend) or offset != len(data):
        raise EvidenceError(f"PNG is incomplete or has trailing bytes: {path}")
    return width, height


def require_sha256(value: str, name: str) -> None:
    if re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise EvidenceError(f"{name} is not a lowercase SHA-256 digest")


def marker_pid(path: Path) -> int:
    try:
        lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line]
    except OSError as error:
        raise EvidenceError(f"cannot read Iced readiness marker: {error}") from error
    pid_lines = [
        line
        for line in lines
        if re.fullmatch(r"ICED_CAPTURE_MARKER event=process_ready pid=[1-9][0-9]*", line)
    ]
    frame = "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance"
    target = (
        "ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu"
    )
    if len(pid_lines) != 1 or lines.count(frame) != 1 or lines.count(target) != 1:
        raise EvidenceError("Iced marker does not prove one ready Performance/CPU demo process")
    expected = [pid_lines[0], frame, target]
    if lines != expected:
        raise EvidenceError("Iced marker contains unexpected, missing, or duplicate evidence lines")
    return int(pid_lines[0].rsplit("=", 1)[1])


def validate_capture_payload(bundle: Path) -> dict[str, str]:
    metadata = parse_metadata(bundle / "capture-metadata.txt")
    if missing := ICED_REQUIRED - metadata.keys():
        raise EvidenceError(f"Iced metadata missing fields: {sorted(missing)}")
    if metadata["frontend"] != "iced":
        raise EvidenceError(f"unexpected frontend: {metadata['frontend']!r}")
    if metadata["target"] != "windows":
        raise EvidenceError(f"unexpected capture target: {metadata['target']!r}")
    if metadata["capture_api"] != CAPTURE_API:
        raise EvidenceError(f"unexpected capture API: {metadata['capture_api']!r}")
    if metadata["scenario"] != "performance-cpu":
        raise EvidenceError(f"unsupported paired Iced scenario: {metadata['scenario']!r}")
    if metadata["theme"] not in {"windows-dark", "windows-light"}:
        raise EvidenceError(f"unexpected paired theme: {metadata['theme']!r}")
    if metadata["high_contrast"] not in {"0", "1"}:
        raise EvidenceError("high_contrast must be 0 or 1")
    if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{2,127}", metadata["pair_id"]) is None:
        raise EvidenceError("pair_id is empty or contains unsupported characters")
    if metadata["window_title"] != "TaskForestI":
        raise EvidenceError(f"unexpected target window title: {metadata['window_title']!r}")
    if metadata["window_process_name"].lower() != "taskforest-i.exe":
        raise EvidenceError(
            f"unexpected target process name: {metadata['window_process_name']!r}"
        )
    if metadata["source_scope"] != "iced":
        raise EvidenceError(f"unexpected source scope: {metadata['source_scope']!r}")
    if metadata["renderer_active"] != "unknown":
        raise EvidenceError("renderer_active must stay unknown until Iced reports its selection")
    if metadata["text_rasterizer_active"] != "unknown":
        raise EvidenceError("text_rasterizer_active must stay unknown without runtime telemetry")
    if metadata["native_text_api"] != "unknown":
        raise EvidenceError("native_text_api must not claim an unobserved platform API")
    if metadata["font_weight_axis_policy"] != "directwrite-policy":
        raise EvidenceError("unexpected Windows font-weight policy axis")
    if (metadata["font_weight_450_name"], metadata["font_weight_450_value"]) != (
        "Medium",
        "500",
    ):
        raise EvidenceError("Iced Windows weight-450 mapping differs from its owner binding")

    if re.fullmatch(r"[0-9a-f]{12,40}", metadata["git_head"]) is None:
        raise EvidenceError("git_head is missing or malformed")
    if metadata["worktree"] not in {"clean", "dirty"}:
        raise EvidenceError("worktree must be clean or dirty")
    if not metadata["rust"].startswith("rustc "):
        raise EvidenceError("rust provenance is missing")
    pid = metadata["target_pid"]
    if re.fullmatch(r"[1-9][0-9]*", pid) is None:
        raise EvidenceError("target_pid is missing or malformed")

    image = bundle / "capture.png"
    require_sha256(metadata.get("png_sha256", ""), "png_sha256")
    if sha256_file(image) != metadata["png_sha256"]:
        raise EvidenceError("capture.png hash differs from png_sha256")
    if png_dimensions(image) != positive_size(metadata["window_size"], "window_size"):
        raise EvidenceError("capture.png dimensions differ from window_size")
    if metadata["requested_window_size"] != PAIRED_REQUESTED_SIZE:
        raise EvidenceError(
            f"unsupported paired requested window size: {metadata['requested_window_size']!r}"
        )
    positive_size(metadata["window_rect_size"], "window_rect_size")

    marker_path = relative_bundle_path(bundle, metadata["marker_file"], "Iced readiness marker")
    require_sha256(metadata["marker_sha256"], "marker_sha256")
    if sha256_file(marker_path) != metadata["marker_sha256"]:
        raise EvidenceError("Iced readiness marker hash differs from marker_sha256")
    if marker_pid(marker_path) != int(pid):
        raise EvidenceError("target_pid differs from the PID emitted by the Iced app")

    target_path = relative_bundle_path(bundle, metadata["target_metadata"], "Iced target metadata")
    require_sha256(metadata["target_metadata_sha256"], "target_metadata_sha256")
    if sha256_file(target_path) != metadata["target_metadata_sha256"]:
        raise EvidenceError("capture target metadata hash differs from its recorded hash")
    target = parse_metadata(target_path)
    for field in (
        "capture_api",
        "target_pid",
        "window_process_name",
        "window_title",
        "window_size",
        "window_rect_size",
        "renderer_active",
        "native_text_api",
        "text_shaping",
        "text_rasterizer",
        "text_rasterizer_active",
        "font_weight_axis_policy",
        "font_weight_450_name",
        "font_weight_450_value",
    ):
        if target.get(field) != metadata[field]:
            raise EvidenceError(f"capture target metadata disagrees on {field}")
    return metadata


def relative_repo_path(root: Path, value: str, name: str) -> Path:
    candidate = Path(value)
    if candidate.is_absolute() or PureWindowsPath(value).is_absolute():
        raise EvidenceError(f"{name} must be a repository-relative path")
    resolved = (root / candidate).resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError as error:
        raise EvidenceError(f"{name} escapes the repository: {value}") from error
    return resolved


def relative_bundle_path(bundle: Path, value: str, name: str) -> Path:
    candidate = Path(value)
    if candidate.is_absolute() or PureWindowsPath(value).is_absolute():
        raise EvidenceError(f"{name} must be relative to the evidence bundle")
    resolved = (bundle / candidate).resolve()
    try:
        resolved.relative_to(bundle.resolve())
    except ValueError as error:
        raise EvidenceError(f"{name} escapes the evidence bundle: {value}") from error
    return resolved


def validate_source_manifest(path: Path, root: Path, frontend: str) -> None:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise EvidenceError(f"cannot read source manifest: {error}") from error
    if not lines:
        raise EvidenceError("source manifest is empty")
    allowed_external: set[Path] | None = None
    seen: set[Path] = set()
    for number, line in enumerate(lines, 1):
        digest, separator, relative = line.partition("  ")
        if not separator or not relative:
            raise EvidenceError(f"invalid source manifest line {number}")
        require_sha256(digest, f"source manifest line {number} digest")
        source = (root / relative).resolve()
        try:
            source.relative_to(root.resolve())
        except ValueError:
            if allowed_external is None:
                try:
                    from frontend_source_manifest import source_paths

                    allowed_external = {
                        entry.resolve() for entry in source_paths(root, frontend)
                    }
                except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired, ValueError) as error:
                    raise EvidenceError(f"cannot resolve allowed source scope: {error}") from error
            if source not in allowed_external:
                raise EvidenceError(f"source path is outside {frontend} production scope: {relative}")
        if source in seen:
            raise EvidenceError(f"duplicate source manifest path: {relative}")
        seen.add(source)
        if not source.is_file() or sha256_file(source) != digest:
            raise EvidenceError(f"source provenance mismatch: {relative}")


def locked_package(packages: list[dict], name: str, version: str | None = None) -> dict:
    matches = [
        package
        for package in packages
        if package["name"] == name and (version is None or package["version"] == version)
    ]
    if len(matches) != 1:
        raise EvidenceError(
            f"Cargo.lock does not resolve exactly one {name} {version or ''} package"
        )
    return matches[0]


def locked_dependency(packages: list[dict], parent: dict, name: str) -> dict:
    entries = [
        dependency
        for dependency in parent.get("dependencies", [])
        if dependency.split(" ", 1)[0] == name
    ]
    if len(entries) != 1:
        raise EvidenceError(f"Cargo.lock has no unique {name} edge from {parent['name']}")
    parts = entries[0].split()
    version = parts[1] if len(parts) > 1 and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+.*", parts[1]) else None
    return locked_package(packages, name, version)


def resolved_renderer_text_stack(lock_path: Path) -> tuple[str, str]:
    try:
        packages = tomllib.loads(lock_path.read_text(encoding="utf-8"))["package"]
    except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
        raise EvidenceError(f"cannot parse Cargo.lock: {error}") from error
    iced = locked_package(packages, "iced", "0.14.0")
    renderer = locked_dependency(packages, iced, "iced_renderer")
    wgpu_renderer = locked_dependency(packages, renderer, "iced_wgpu")
    tiny_skia_renderer = locked_dependency(packages, renderer, "iced_tiny_skia")
    wgpu = locked_dependency(packages, wgpu_renderer, "wgpu")
    graphics = locked_dependency(packages, wgpu_renderer, "iced_graphics")
    rasterizer = locked_dependency(packages, wgpu_renderer, "cryoglyph")
    cosmic_text = locked_dependency(packages, graphics, "cosmic-text")
    fontdb = locked_dependency(packages, cosmic_text, "fontdb")
    shaper = locked_dependency(packages, cosmic_text, "harfrust")
    swash = locked_dependency(packages, cosmic_text, "swash")
    renderer_stack = ";".join(
        f"{package['name']}@{package['version']}"
        for package in (renderer, wgpu_renderer, tiny_skia_renderer, wgpu)
    )
    text_stack = ";".join(
        f"{package['name']}@{package['version']}"
        for package in (rasterizer, graphics, cosmic_text, fontdb, shaper, swash)
    )
    return renderer_stack, text_stack


def current(*command: str, cwd: Path) -> str:
    try:
        return subprocess.run(
            command,
            cwd=cwd,
            check=True,
            text=True,
            capture_output=True,
            timeout=15,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise EvidenceError(f"cannot collect current provenance {command[0]}: {error}") from error


def validate_peer(bundle: Path, metadata: dict[str, str], root: Path) -> None:
    pair_root = bundle.resolve().parent

    def peer_path(field: str) -> Path:
        value = Path(metadata[field])
        if value.is_absolute() or PureWindowsPath(metadata[field]).is_absolute():
            raise EvidenceError(f"{field} must be relative to the Iced receipt")
        resolved = (bundle / value).resolve()
        try:
            resolved.relative_to(pair_root)
        except ValueError as error:
            raise EvidenceError(f"{field} escapes the pair directory") from error
        return resolved

    peer_receipt_path = peer_path("peer_receipt")
    peer_capture_path = peer_path("peer_capture_metadata")
    peer_image_path = peer_path("peer_image")
    peer_manifest_path = peer_path("peer_source_manifest")
    peer = parse_metadata(peer_receipt_path)
    if missing := GPUI_REQUIRED - peer.keys():
        raise EvidenceError(f"GPUI peer receipt missing fields: {sorted(missing)}")
    if peer["window_size"] != parse_metadata(peer_capture_path).get("window_size"):
        raise EvidenceError("GPUI receipt and capture metadata disagree on actual dimensions")
    for field, expected in (
        ("frontend", "gpui"),
        ("target", "windows"),
        ("scenario", metadata["scenario"]),
        ("pair_id", metadata["pair_id"]),
        ("theme", metadata["theme"]),
        ("high_contrast", metadata["high_contrast"]),
        ("git_head", metadata["git_head"]),
        ("worktree", metadata["worktree"]),
        ("rust", metadata["rust"]),
    ):
        if peer[field] != expected:
            raise EvidenceError(f"GPUI peer differs from Iced on {field}")
    if peer["requested_window_size"] != metadata["requested_window_size"]:
        raise EvidenceError("GPUI and Iced requested different window dimensions")
    for field, expected in (
        ("peer_frontend", peer["frontend"]),
        ("peer_target", peer["target"]),
        ("peer_scenario", peer["scenario"]),
        ("peer_theme", peer["theme"]),
        ("peer_high_contrast", peer["high_contrast"]),
        ("peer_pair_id", peer["pair_id"]),
        ("peer_requested_window_size", peer["requested_window_size"]),
        ("peer_window_size", peer["window_size"]),
        ("peer_binary_sha256", peer["binary_sha256"]),
        ("peer_source_manifest_sha256", peer["source_manifest_sha256"]),
    ):
        if metadata[field] != expected:
            raise EvidenceError(f"Iced receipt has stale peer summary field {field}")

    peer_capture = parse_metadata(peer_capture_path)
    if "window_size" not in peer_capture or "png_sha256" not in peer_capture:
        raise EvidenceError("GPUI capture metadata lacks actual dimensions or PNG hash")
    iced_size = positive_size(metadata["window_size"], "Iced window_size")
    gpui_size = positive_size(peer_capture["window_size"], "GPUI window_size")
    if iced_size != gpui_size:
        raise EvidenceError(
            f"paired Windows images have different actual dimensions: Iced {iced_size}, GPUI {gpui_size}"
        )
    require_sha256(peer_capture["png_sha256"], "GPUI png_sha256")
    if metadata["peer_png_sha256"] != peer_capture["png_sha256"]:
        raise EvidenceError("Iced receipt has a stale peer PNG hash")
    if sha256_file(peer_image_path) != peer_capture["png_sha256"]:
        raise EvidenceError("GPUI capture.png hash differs from its capture metadata")
    if png_dimensions(peer_image_path) != gpui_size:
        raise EvidenceError("GPUI PNG dimensions differ from its capture metadata")

    peer_binary = relative_repo_path(root, peer["binary"], "GPUI binary")
    if peer_binary.name.lower() != "taskforest-g.exe":
        raise EvidenceError("GPUI peer binary identity is not taskforest-g.exe")
    require_sha256(peer["binary_sha256"], "GPUI binary_sha256")
    if sha256_file(peer_binary) != peer["binary_sha256"]:
        raise EvidenceError("GPUI peer binary hash differs from its receipt")
    peer_manifest = peer_manifest_path
    require_sha256(peer["source_manifest_sha256"], "GPUI source_manifest_sha256")
    if sha256_file(peer_manifest) != peer["source_manifest_sha256"]:
        raise EvidenceError("GPUI source manifest hash differs from its receipt")
    validate_source_manifest(peer_manifest, root, "gpui")


def validate_bundle(bundle: Path, repo_root: Path, *, check_current: bool = True) -> dict[str, str]:
    bundle = bundle.resolve()
    root = repo_root.resolve()
    metadata = validate_capture_payload(bundle)

    for field, name in (
        ("png_sha256", "Iced PNG hash"),
        ("binary_sha256", "Iced binary hash"),
        ("cargo_lock_sha256", "Cargo.lock hash"),
        ("source_manifest_sha256", "Iced source manifest hash"),
        ("marker_sha256", "Iced marker hash"),
        ("target_metadata_sha256", "Iced target metadata hash"),
    ):
        require_sha256(metadata[field], name)

    lock_path = root / "Cargo.lock"
    if sha256_file(lock_path) != metadata["cargo_lock_sha256"]:
        raise EvidenceError("Cargo.lock hash differs from the Iced receipt")
    renderer_stack, text_stack = resolved_renderer_text_stack(lock_path)
    if metadata["renderer_stack"] != renderer_stack:
        raise EvidenceError("renderer stack differs from the current Cargo.lock resolution")
    if metadata["text_stack"] != text_stack:
        raise EvidenceError("text stack differs from the current Cargo.lock resolution")
    if metadata["text_shaping"] != "cosmic-text Advanced Auto default; widget overrides may vary":
        raise EvidenceError("unexpected Iced shaping path declaration")
    if metadata["text_rasterizer"] != (
        "cryoglyph for WGPU; cosmic-text+swash for tiny-skia fallback"
    ):
        raise EvidenceError("unexpected Iced text rasterizer path declaration")

    binary = relative_repo_path(root, metadata["binary"], "Iced binary")
    if binary.name.lower() != "taskforest-i.exe":
        raise EvidenceError("Iced binary identity is not taskforest-i.exe")
    if sha256_file(binary) != metadata["binary_sha256"]:
        raise EvidenceError("Iced binary hash differs from its receipt")
    source_manifest = relative_bundle_path(
        bundle, metadata["source_manifest"], "Iced source manifest"
    )
    if sha256_file(source_manifest) != metadata["source_manifest_sha256"]:
        raise EvidenceError("Iced source manifest hash differs from its receipt")
    validate_source_manifest(source_manifest, root, "iced")

    validate_peer(bundle, metadata, root)
    if check_current:
        head = current("git", "rev-parse", "--short=12", "HEAD", cwd=root)
        rust = current("rustc", "-V", cwd=root)
        worktree = "dirty" if current("git", "status", "--porcelain", cwd=root) else "clean"
        if (head, rust, worktree) != (
            metadata["git_head"],
            metadata["rust"],
            metadata["worktree"],
        ):
            raise EvidenceError("capture provenance differs from the current git/rust/worktree state")
    return metadata


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, required=True)
    args = parser.parse_args()
    try:
        values = validate_bundle(args.bundle, args.repo_root)
    except (EvidenceError, OSError, ValueError, KeyError) as error:
        print(f"Iced Windows evidence: FAIL: {error}", file=sys.stderr)
        return 1
    print(
        "Iced Windows evidence: PASS "
        f"pair={values['pair_id']} size={values['window_size']} "
        f"png_sha256={values['png_sha256']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
