#!/usr/bin/env python3
"""zhuge-ui-kit Bevy 场景门禁：BSN 唯一构造路线 + 尺寸字面量唯一 rem 路线。

本脚本是 ``crates/zhuge-ui-kit-bevy`` README「Contract」段那两条律法的机械执行器：
律法正文只有一份（crate README），这里是它的唯一实现。脚本随 crate 一起发布，
消费者从自己锁定的 crate 版本里取出同一份文件执行，规则只有一份实现；
各产品只持有自己的存量台账（棘轮基线）。

律法一（BSN 唯一构造路线）
    UI 层级只能在 ``bsn! { ... }`` 场景内部声明；手动挂子 API、直接 spawn、
    ``template_value`` 硬零与失衡括号一律失败。

律法二（尺寸字面量唯一 rem 路线）
    生产 Bevy 源码里不得出现硬编码的非零 px 字面量（``px(4.0)`` /
    ``Val::Px(4.0)``）；``0`` 复位、命名 token（发丝线、光标、断点、最小触控
    目标）与测量换算结果合法。字号必须是 ``FontSize::Rem``。

用法::

    python3 tools/check_bevy_ui.py                        # 扫描本 crate 的 src（默认强制）
    python3 tools/check_bevy_ui.py --mode report          # 只报告不失败
    python3 tools/check_bevy_ui.py --root <path>          # 扫描消费者 Bevy UI 源码树
    python3 tools/check_bevy_ui.py --write-baseline b.json # 记录当前存量（棘轮台账）
    python3 tools/check_bevy_ui.py --baseline b.json      # 存量放行、新增违规失败
    python3 tools/check_bevy_ui.py --self-test            # 内建自检

基线（棘轮）语义：基线里的旧账放行，任何基线之外的新违规立即失败。基线指纹是
``路径|规则码|归一化源码行``，行号漂移不会让基线失效；基线里已不存在的旧账
按信息报出（存量还清了就该收基线）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

BASELINE_VERSION = 1

BSN_START = re.compile(r"\bbsn!\s*\{")
STRUCTURE_PRIMITIVE = re.compile(
    r"\b(?:Node|Children|Text)\s*(?:\{|\[|\()"
    r"|\b(?:NodeBundle|TextBundle|ButtonBundle|ImageBundle)\b"
)
STRUCTURE_TEMP = re.compile(
    r"(?<!accesskit::)\b(?:Node|Children|Text)::(?:new|default|EMPTY|from\w*)\b"
)
MANUAL_CHILD_API = re.compile(
    r"\.\s*(?:with_children|push_children|add_child|add_children"
    r"|insert_children|replace_children)\s*\("
)
DIRECT_SPAWN = re.compile(r"\.\s*spawn(?:_batch|_empty|_empty_at)?\s*\(")
ALLOWED_SPAWN_ARGUMENTS = ("Camera2d", "Observer::new")
TEMPLATE_VALUE = re.compile(r"\btemplate_value\b|\bTemplateValue\b")
PX_VARIANT = re.compile(r"\bVal::Px\s*\(\s*(?P<sign>-?)(?P<num>\d+(?:\.\d+)?)\s*\)")
PX_HELPER = re.compile(r"(?<!\w)px\s*\(\s*(?P<sign>-?)(?P<num>\d+(?:\.\d+)?)\s*\)")
FONT_SIZE_PX = re.compile(r"\bFontSize::Px\b")


@dataclass(frozen=True)
class Violation:
    """One mechanical violation of the Bevy scene contract."""

    path: str
    line: int
    code: str
    detail: str
    snippet: str

    def fingerprint(self) -> str:
        """Baseline key: path, rule code and the normalized source line."""

        return f"{self.path}|{self.code}|{self.snippet}"


def _blank(chars: list[str], start: int, end: int) -> None:
    """Blank a non-code range while keeping line endings and offsets."""

    for index in range(start, min(end, len(chars))):
        if chars[index] not in "\r\n":
            chars[index] = " "


def mask_noncode(text: str) -> str:
    """Mask comments and literals without changing offsets or line numbers.

    Documentation examples and prose must neither satisfy nor trip a rule.
    """

    chars = list(text)
    length = len(text)
    index = 0
    block_depth = 0
    while index < length:
        pair = text[index : index + 2]
        if block_depth:
            if pair == "/*":
                _blank(chars, index, index + 2)
                block_depth += 1
                index += 2
            elif pair == "*/":
                _blank(chars, index, index + 2)
                block_depth -= 1
                index += 2
            else:
                _blank(chars, index, index + 1)
                index += 1
            continue

        if pair == "//":
            end = text.find("\n", index)
            end = length if end < 0 else end
            _blank(chars, index, end)
            index = end
            continue
        if pair == "/*":
            _blank(chars, index, index + 2)
            block_depth = 1
            index += 2
            continue

        raw = re.match(r"(?:br|r)(#+)?\"", text[index:])
        if raw:
            hashes = raw.group(1) or ""
            content_start = index + len(raw.group(0))
            terminator = f'"{hashes}'
            end = text.find(terminator, content_start)
            end = length if end < 0 else end + len(terminator)
            _blank(chars, index, end)
            index = end
            continue

        if text[index] == '"':
            end = index + 1
            escaped = False
            while end < length:
                current = text[end]
                if current == "\n" and not escaped:
                    break
                if current == '"' and not escaped:
                    end += 1
                    break
                if current == "\\":
                    escaped = not escaped
                else:
                    escaped = False
                end += 1
            _blank(chars, index, end)
            index = end
            continue

        if text[index] == "'":
            end = index + 1
            escaped = False
            while end < length and text[end] not in "\r\n":
                current = text[end]
                if current == "'" and not escaped:
                    end += 1
                    _blank(chars, index, end)
                    index = end
                    break
                if current == "\\":
                    escaped = not escaped
                else:
                    escaped = False
                end += 1
            else:
                index += 1
            continue

        index += 1
    return "".join(chars)


def line_number(text: str, offset: int) -> int:
    """1-based line number of ``offset`` inside ``text``."""

    return text.count("\n", 0, offset) + 1


def source_line(text: str, offset: int) -> str:
    """Normalized source line of ``offset`` (used as the baseline fingerprint)."""

    start = text.rfind("\n", 0, offset) + 1
    end = text.find("\n", offset)
    end = len(text) if end < 0 else end
    return " ".join(text[start:end].split())[:160]


def scene_spans(masked: str) -> tuple[list[tuple[int, int]], list[int]]:
    """Balanced ``bsn! { ... }`` spans plus the offsets of unbalanced starts."""

    spans: list[tuple[int, int]] = []
    unbalanced: list[int] = []
    for match in BSN_START.finditer(masked):
        opening = masked.find("{", match.start(), match.end())
        depth = 0
        closing = None
        for index in range(opening, len(masked)):
            if masked[index] == "{":
                depth += 1
            elif masked[index] == "}":
                depth -= 1
                if depth == 0:
                    closing = index + 1
                    break
        if closing is None:
            unbalanced.append(match.start())
        else:
            spans.append((match.start(), closing))
    return spans, unbalanced


def inside_scene(offset: int, spans: list[tuple[int, int]]) -> bool:
    """True when ``offset`` sits inside a balanced ``bsn!`` scene span."""

    return any(start <= offset < end for start, end in spans)


def in_return_position(masked: str, start: int) -> bool:
    """True when a ``Type {`` match follows a ``->`` and is a signature, not a value.

    ``fn row() -> accesskit::Node {`` opens a function body; it does not build
    a UI node. The type may be path-qualified, so the ``::``-separated prefix
    is walked back before the return arrow is inspected.
    """

    prefix = re.search(r"[\w:]+$", masked[:start])
    before = masked[: prefix.start()] if prefix else masked[:start]
    return before.rstrip().endswith("->")


def matching_paren(masked: str, opening: int) -> int | None:
    """Index of the parenthesis closing ``opening``."""

    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "(":
            depth += 1
        elif masked[index] == ")":
            depth -= 1
            if depth == 0:
                return index
    return None


def is_zero_literal(match: re.Match[str]) -> bool:
    """True when a px literal is a zero reset (``0``, ``0.0``)."""

    try:
        value = float(f"{match.group('sign')}{match.group('num')}")
    except ValueError:
        return False
    return value == 0.0


def analyze(rel_path: str, text: str) -> list[Violation]:
    """Violations of the Bevy scene contract inside one source file."""

    original = text
    masked = mask_noncode(text)
    spans, unbalanced = scene_spans(masked)
    violations: list[Violation] = []

    def report(offset: int, code: str, detail: str) -> None:
        violations.append(
            Violation(
                rel_path,
                line_number(original, offset),
                code,
                detail,
                source_line(original, offset),
            )
        )

    for offset in unbalanced:
        report(offset, "UIK-BSN-004", "bsn! scene has unbalanced braces")

    for match in STRUCTURE_PRIMITIVE.finditer(masked):
        if inside_scene(match.start(), spans) or in_return_position(
            masked, match.start()
        ):
            continue
        report(
            match.start(),
            "UIK-BSN-001",
            "UI hierarchy must be declared inside a bsn! scene",
        )

    for match in STRUCTURE_TEMP.finditer(masked):
        if not inside_scene(match.start(), spans):
            report(
                match.start(),
                "UIK-BSN-006",
                "structure temporary outside a bsn! scene; declare it in the scene",
            )

    for match in MANUAL_CHILD_API.finditer(masked):
        report(
            match.start(),
            "UIK-BSN-002",
            "manual child-link APIs are forbidden; compose a bsn! scene",
        )

    for match in DIRECT_SPAWN.finditer(masked):
        opening = masked.find("(", match.start(), match.end())
        closing = matching_paren(masked, opening)
        arguments = masked[opening + 1 : closing].lstrip() if closing else ""
        if arguments.startswith(ALLOWED_SPAWN_ARGUMENTS):
            continue
        report(
            match.start(),
            "UIK-BSN-003",
            "direct entity spawn is forbidden; mount UI through spawn_scene",
        )

    for match in TEMPLATE_VALUE.finditer(masked):
        report(
            match.start(),
            "UIK-BSN-005",
            "template_value is hard-zero; declare the value structurally",
        )

    for pattern in (PX_VARIANT, PX_HELPER):
        for match in pattern.finditer(masked):
            if is_zero_literal(match):
                continue
            report(
                match.start(),
                "UIK-REM-001",
                "hardcoded px literal is forbidden; use rem/em/percent or a named token",
            )

    for match in FONT_SIZE_PX.finditer(masked):
        report(
            match.start(),
            "UIK-REM-002",
            "font size must be rem-based; FontSize::Px is forbidden",
        )

    return violations


def is_excluded(path: Path) -> bool:
    """Test trees and build output sit outside the production authoring contract.

    Test modules mounted inside the production tree (``*_test.rs`` /
    ``*_tests.rs``) are fixtures, not authoring routes.
    """

    return (
        "target" in path.parts
        or "tests" in path.parts
        or path.stem.endswith(("_test", "_tests"))
    )


def rust_files(root: Path) -> list[Path]:
    """Production Rust files under ``root``."""

    if root.is_file():
        return [root] if root.suffix == ".rs" and not is_excluded(root) else []
    return sorted(
        path
        for path in root.rglob("*.rs")
        if path.is_file() and not is_excluded(path)
    )


def root_label(root: Path) -> str:
    """Stable label of a scanned tree, used as the fingerprint path prefix.

    The label never contains an absolute path, so a baseline recorded on one
    machine keeps working on another.
    """

    resolved = root.resolve()
    if resolved.is_file():
        return resolved.name
    return f"{resolved.parent.name}/{resolved.name}"


def repo_root() -> Path:
    """The repository that owns the running gate.

    The gate ships inside the crate and is vendored by consumers, so the
    checkout root is found by walking up from the script rather than assumed
    to be a fixed number of parent directories.
    """

    for candidate in Path(__file__).resolve().parents:
        if (candidate / ".git").exists():
            return candidate
    return Path.cwd()


def default_roots() -> list[Path]:
    """The adapter's own source tree, resolved from the script location.

    The gate ships inside the crate, so the crate's ``src`` directory is the
    default target both in this repository and in a consumer checkout.
    """

    crate_src = Path(__file__).resolve().parent.parent / "src"
    return [crate_src] if crate_src.is_dir() else []


def scan(roots: list[Path]) -> list[Violation]:
    """All violations under ``roots``."""

    violations: list[Violation] = []
    for root in roots:
        label = root_label(root)
        for path in rust_files(root):
            text = path.read_text(encoding="utf-8", errors="replace")
            if root.is_file():
                relative = path.name
            else:
                relative = path.relative_to(root).as_posix()
            violations.extend(analyze(f"{label}/{relative}", text))
    return violations


def load_baseline(path: Path) -> dict[str, int]:
    """Load a ratchet baseline; a missing or malformed file is a hard error."""

    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("version") != BASELINE_VERSION:
        raise ValueError(f"unsupported baseline version: {payload.get('version')!r}")
    entries = payload.get("entries")
    if not isinstance(entries, dict):
        raise ValueError("baseline entries must be an object")
    return {str(key): int(value) for key, value in entries.items()}


def gate_sha256() -> str:
    """SHA-256 of the running gate file."""

    return hashlib.sha256(Path(__file__).resolve().read_bytes()).hexdigest()


def provenance_mismatch(path: Path) -> str | None:
    """The recorded gate digest of a ledger, when it differs from this file.

    A ledger records the digest of the gate it was written against. The
    product that owns the ledger runs the vendored copy of that exact gate, so
    a mismatch there means the copy was edited without refreshing the ledger.
    A newer gate run against an older ledger is a normal upgrade path, so the
    difference is reported and left to ``--strict-provenance``.
    """

    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    recorded = payload.get("gate", {})
    if not isinstance(recorded, dict):
        return None
    expected = recorded.get("sha256")
    if not expected:
        return None
    actual = gate_sha256()
    return None if expected == actual else f"{expected} != {actual}"


def write_baseline(path: Path, violations: list[Violation]) -> int:
    """Write the current violations as the ratchet baseline."""

    counts = Counter(violation.fingerprint() for violation in violations)
    payload = {
        "version": BASELINE_VERSION,
        "gate": {
            "source": "zhuge-ui-kit:crates/zhuge-ui-kit-bevy/tools/check_bevy_ui.py",
            "sha256": gate_sha256(),
        },
        "entries": dict(sorted(counts.items())),
    }
    path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return len(counts)


def split_by_baseline(
    violations: list[Violation], baseline: dict[str, int]
) -> tuple[list[Violation], list[str]]:
    """Split violations into (new, covered) against a ratchet baseline."""

    covered = Counter()
    new: list[Violation] = []
    for violation in violations:
        key = violation.fingerprint()
        if covered[key] < baseline.get(key, 0):
            covered[key] += 1
        else:
            new.append(violation)
    stale = sorted(key for key, count in baseline.items() if covered[key] < count)
    return new, stale


def self_test() -> int:
    """Positive and negative cases, inline, without touching the repository."""

    cases: list[tuple[str, str, list[str]]] = [
        (
            "bsn! scene is the sanctioned hierarchy route",
            "fn scene() { bsn! { Node { width: rem(1.0) } Children [ Text(label) ] } }",
            [],
        ),
        (
            "structure outside a scene is banned",
            "let node = Node { width: rem(1.0) };",
            ["UIK-BSN-001"],
        ),
        (
            "manual child-link API is banned",
            "commands.entity(root).with_children(|_| {});",
            ["UIK-BSN-002"],
        ),
        (
            "direct spawn is banned",
            "commands.spawn(Button);",
            ["UIK-BSN-003"],
        ),
        (
            "camera and observer infrastructure stay allowed",
            "fn setup(mut commands: Commands) { commands.spawn(Camera2d); }",
            [],
        ),
        (
            "unbalanced scene braces fail safe",
            "fn broken() { bsn! {\n",
            ["UIK-BSN-004"],
        ),
        (
            "template_value is hard-zero",
            "let wrapped = template_value(transform);",
            ["UIK-BSN-005"],
        ),
        (
            "structure temporary outside a scene is banned",
            "let node = Node::default();",
            ["UIK-BSN-006"],
        ),
        (
            "hardcoded px literal is banned",
            "fn scene() { bsn! { Node { width: px(4.0) } } }",
            ["UIK-REM-001"],
        ),
        (
            "px variant literal is banned",
            "fn scene() { bsn! { Node { width: Val::Px(4.0) } } }",
            ["UIK-REM-001"],
        ),
        (
            "zero reset is allowed",
            "fn scene() { bsn! { Node { min_width: px(0.0) } } }",
            [],
        ),
        (
            "named token and measured values are allowed",
            "fn scene() { bsn! { Node { width: px(palette.hairline_px) } } }",
            [],
        ),
        (
            "font size must be rem-based",
            "fn scene() { bsn! { TextFont { font_size: FontSize::Px(12.0) } } }",
            ["UIK-REM-002"],
        ),
        (
            "comments and literals are masked",
            '// px(4.0) Node { } commands.spawn(Foo)\nlet doc = "px(4.0)";',
            [],
        ),
    ]

    ok = True
    for label, source, expected in cases:
        found = sorted(violation.code for violation in analyze("self-test.rs", source))
        if found == sorted(expected):
            print(f"  [ok] {label}")
        else:
            ok = False
            print(f"  [FAIL] {label}: expected {sorted(expected)}, found {found}")
    print("self-test:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument(
        "--root",
        type=Path,
        action="append",
        help="scan this file or directory instead of the default roots",
    )
    parser.add_argument("--baseline", type=Path, help="ratchet baseline file")
    parser.add_argument(
        "--write-baseline",
        type=Path,
        help="write the current violations as the ratchet baseline and exit",
    )
    parser.add_argument(
        "--strict-provenance",
        action="store_true",
        help="fail when the ledger was written against a different gate file",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    repo = repo_root()
    roots = list(args.root) if args.root else default_roots()
    roots = [root if root.is_absolute() else repo / root for root in roots]
    if not roots:
        print(
            "bevy ui guard: no scan root; pass --root <tree> or run the gate "
            "from the crate that ships it",
            file=sys.stderr,
        )
        return 2
    violations = scan(roots)

    if args.write_baseline:
        written = write_baseline(args.write_baseline, violations)
        print(f"bevy ui guard: baseline written with {written} entries -> {args.write_baseline}")
        return 0

    baseline: dict[str, int] = {}
    if args.baseline:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"bevy ui guard: baseline unreadable: {error}", file=sys.stderr)
            return 2
        mismatch = provenance_mismatch(args.baseline)
        if mismatch:
            message = (
                "bevy ui guard: ledger was written against a different gate "
                f"({mismatch}); refresh the vendored gate and the ledger together"
            )
            if args.strict_provenance:
                print(message, file=sys.stderr)
                return 1
            print(f"{message} (advisory)", file=sys.stderr)

    new, stale = split_by_baseline(violations, baseline)
    for violation in new:
        print(
            f"{violation.path}:{violation.line}: {violation.code}: {violation.detail}",
            file=sys.stderr,
        )
    for key in stale:
        print(f"bevy ui guard: stale baseline entry: {key}", file=sys.stderr)

    scanned = sum(len(rust_files(root)) for root in roots)
    if scanned == 0:
        print(
            "bevy ui guard: no Rust source scanned — refusing to pass silently",
            file=sys.stderr,
        )
        return 1
    covered = len(violations) - len(new)
    print(
        f"bevy ui guard: scanned={scanned} violations={len(violations)} "
        f"new={len(new)} baselined={covered} stale={len(stale)}"
    )
    if new and args.mode == "enforce":
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
