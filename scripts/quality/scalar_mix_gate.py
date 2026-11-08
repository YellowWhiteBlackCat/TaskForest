#!/usr/bin/env python3
"""zhuge-ui-core 标量线性混合唯一性门禁：`A + (B - A) * t` 只有一个 owner。

正文只有一份：``crates/zhuge-ui-core/README.md`` 的「Law — the scalar mix has one owner」。
本文件是它的唯一机械执行器，随 crate 一起发布；消费者从自己锁定的 crate 版本里取出
同一份文件执行，规则实现只有一份，各产品只持有自己的存量台账（棘轮基线）。

律法一 UIK-MIX-001（不得手写标量混合）
    ``A + (B - A) * t``（或其交换写法 ``t * (B - A) + A``）就是
    ``zhuge_ui_core::easing::lerp``。在**能调用 owner 的 crate**（Cargo.toml 已声明
    zhuge-ui-core，或它本身就是 owner）里再写一遍，即同一事实的第二实现。进度的钳制、
    NaN 规则与结果域钳制是调用方的**策略**，留在原地：``lerp(a, b, t.clamp(0.0, 1.0))``
    合法，本地包装函数保留，只要它的算术来自 owner。

判定的边界（写死，避免门把布局/颜色判成违规）：
    * **逐分量混合放行**：``s.red + (e.red - s.red) * t`` 这类带字段访问的写法是"这个类型
      自己的混合面"，仅凭文本无法与标量混合区分；该族由普查台账（TODO-144）跟踪，不进门。
    * **未依赖 owner 的 crate 放行**：一个 crate 若没在 Cargo.toml 里声明 ``zhuge-ui-core``，
      它没有可调用的 owner；这条边界是**依赖方向**事实，由最近的 Cargo.toml 机械判定，
      并须在台账/TODO 里写明"该层是否应当依赖共享包"（MF 的 contract/shared、kit 的
      tokens 即属此类）。
    * **owner 自身放行**：同时满足「路径属于 ``zhuge-ui-core``` 与「本文件定义
      ``lerp```」；消费者复制一份定义不获得豁免。
    注释与字符串在匹配前掩掉：说明文字既不得触发也不得豁免。

用法::

    python3 tools/check_scalar_mix.py                        # 扫本仓库各 crate 的 src
    python3 tools/check_scalar_mix.py --mode report --root <tree>
    python3 tools/check_scalar_mix.py --write-baseline b.json
    python3 tools/check_scalar_mix.py --baseline b.json
    python3 tools/check_scalar_mix.py --self-test
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
GATE_SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_scalar_mix.py"
OWNER_CRATE = "zhuge-ui-core"
# Both `zhuge-ui-core = "0.1.13"` and the inherited `zhuge-ui-core.workspace = true` form.
OWNER_DEPENDENCY = re.compile(r"^\s*zhuge-ui-core(?:\.\w+)?\s*=", re.MULTILINE)
OWNER_DEFINITION = re.compile(r"\bpub const fn lerp\b|\bpub fn lerp\b")
CRATE_NAME = re.compile(r'^name\s*=\s*"zhuge-ui-core"\s*$', re.MULTILINE)
OWNER_CALL = re.compile(r"(?<!fn )\blerp\s*\(")
MIX_SPelling = re.compile(
    r"(?P<a>[A-Za-z_][\w.]*)\s*\+\s*\(\s*(?P<b>[A-Za-z_][\w.]*)\s*-\s*(?P=a)\s*\)\s*\*\s*\S"
    r"|(?P<t2>[A-Za-z_][\w.]*|\d[\w.]*(?:\([^()]*\))?)\s*\*\s*\(\s*(?P<b2>[A-Za-z_][\w.]*)\s*-\s*(?P<a2>[A-Za-z_][\w.]*)\s*\)\s*\+\s*(?P=a2)"
)
FIELD_ACCESS = re.compile(r"[A-Za-z_]\w*\s*\.\s*\w")
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r"\"(?:[^\"\\\n]|\\.)*\"|'(?:[^'\\\n]|\\.)'")

SKIP_DIRECTORIES = {".git", "target", "node_modules", ".private", "dist", "build"}


@dataclass
class Violation:
    """One breach of the scalar-mix law, addressed by file, line and text."""

    path: str
    line: int
    code: str
    symbol: str
    detail: str
    snippet: str

    def fingerprint(self) -> str:
        """Baseline key: path, rule code, owning function and source text.

        Two byte-identical breaches in two functions are two copies of the fact,
        so the function is part of the key rather than an omitted detail.
        """

        return f"{self.path}|{self.code}|{self.symbol}|{self.snippet}"


FN_HEADER = re.compile(r"\bfn\s+[A-Za-z_]\w*")
FN_NAME = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
TOP_LEVEL = "<file>"


def enclosing(lines: list[str], line: int) -> str:
    """The function a breach line belongs to.

    Brace counting is unusable in files embedding the Bevy scene DSL, whose
    interpolations and `--` separators no Rust brace model balances. The owner
    is the nearest preceding `fn` header with a shallower indent.
    """

    if line < 1 or line > len(lines):
        return TOP_LEVEL
    breach = lines[line - 1]
    breach_indent = len(breach) - len(breach.lstrip())
    for index in range(line - 1, -1, -1):
        candidate = lines[index]
        if not FN_HEADER.search(candidate):
            continue
        indent = len(candidate) - len(candidate.lstrip())
        if breach_indent <= indent and index != line - 1:
            return TOP_LEVEL
        name = FN_NAME.search(candidate)
        return name.group(1) if name else TOP_LEVEL
    return TOP_LEVEL


def mask_noncode(text: str) -> str:
    """Blank comments and string literals while keeping offsets and lines."""

    def blank(match: re.Match[str]) -> str:
        return " " * len(match.group(0)) if "\n" not in match.group(0) else match.group(0)

    masked = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.DOTALL)
    masked = LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), masked)
    return STRING_LITERAL.sub(blank, masked)


def normalize(line: str) -> str:
    text = " ".join(line.split())
    if len(text) > 160:
        return text[:157] + "..."
    return text


def statements(text: str) -> list[tuple[int, str]]:
    """Group source lines into bracket-balanced statements."""

    grouped: list[tuple[int, str]] = []
    depth = 0
    start_line = 1
    buffer: list[str] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not buffer and not line.strip():
            continue
        if not buffer:
            start_line = number
        buffer.append(line)
        depth += line.count("(") + line.count("[") + line.count("{")
        depth -= line.count(")") + line.count("]") + line.count("}")
        depth = max(depth, 0)
        if depth == 0 and line.rstrip().endswith((";", "}", "{")):
            grouped.append((start_line, "\n".join(buffer)))
            buffer = []
    if buffer:
        grouped.append((start_line, "\n".join(buffer)))
    return grouped


def plain_scalar_match(statement: str) -> re.Match[str] | None:
    """The mix spelling whose operands are plain names, not component accesses.

    A per-channel color or vector mix is that type's own face and cannot be told
    apart from the scalar rule by text alone, so the gate keeps out of it; a mix of
    two plain values has no such face and must call the owner.
    """

    for match in MIX_SPelling.finditer(statement):
        operands = [
            value
            for value in match.groupdict().values()
            if value
        ]
        if any(FIELD_ACCESS.search(operand) for operand in operands):
            continue
        return match
    return None


def analyze(path: str, text: str, consumes_owner: bool) -> list[Violation]:
    """Every scalar mix re-derived outside the owner."""

    masked = mask_noncode(text)
    if OWNER_CRATE in path and OWNER_DEFINITION.search(masked):
        return []
    if not consumes_owner:
        return []
    found: list[Violation] = []
    seen: set[tuple[str, str]] = set()
    lines = masked.splitlines()

    def record(line: int, code: str, detail: str, snippet: str) -> None:
        symbol = enclosing(lines, line)
        key = (symbol, snippet)
        if key in seen:
            return
        seen.add(key)
        found.append(
            Violation(
                path=path,
                line=line,
                code=code,
                symbol=symbol,
                detail=detail,
                snippet=snippet,
            )
        )

    for line, statement in statements(masked):
        if OWNER_CALL.search(statement):
            continue
        match = plain_scalar_match(statement)
        if match is None:
            continue
        snippet_line = next(
            (raw for raw in statement.splitlines() if MIX_SPelling.search(raw)), statement
        )
        record(
            line,
            "UIK-MIX-001",
            (
                "a scalar mix is spelled by hand; call zhuge_ui_core::easing::lerp and keep the "
                "progress policy (its own clamp) at the call site"
            ),
            normalize(snippet_line),
        )
    return found


def crate_declares_owner(source: Path) -> bool:
    """Whether the crate this file belongs to can reach the owner.

    That is either a declared dependency on `zhuge-ui-core` or being the owner
    crate itself, which is bound by its own law outside the definition file.
    """

    for parent in source.resolve().parents:
        manifest = parent / "Cargo.toml"
        if manifest.is_file():
            text = manifest.read_text(encoding="utf-8", errors="replace")
            if OWNER_DEPENDENCY.search(text) or CRATE_NAME.search(text):
                return True
            return False
    return False


def rust_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix == ".rs" else []
    return sorted(
        path
        for path in root.rglob("*.rs")
        if not SKIP_DIRECTORIES.intersection(path.parts) and path.is_file()
    )


def root_label(root: Path) -> str:
    resolved = root.resolve()
    if resolved.is_file():
        return resolved.name
    return f"{resolved.parent.name}/{resolved.name}"


def scan(roots: list[Path]) -> list[Violation]:
    violations: list[Violation] = []
    for root in roots:
        label = root_label(root)
        for path in rust_files(root):
            text = path.read_text(encoding="utf-8", errors="replace")
            relative = path.name if root.is_file() else path.relative_to(root).as_posix()
            violations.extend(analyze(f"{label}/{relative}", text, crate_declares_owner(path)))
    return violations


def load_baseline(path: Path) -> dict[str, int]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("version") != BASELINE_VERSION:
        raise ValueError(f"unsupported baseline version: {payload.get('version')!r}")
    entries = payload.get("entries")
    if not isinstance(entries, dict):
        raise ValueError("baseline entries must be an object")
    return {str(key): int(value) for key, value in entries.items()}


def gate_sha256() -> str:
    return hashlib.sha256(Path(__file__).resolve().read_bytes()).hexdigest()


def provenance_mismatch(path: Path) -> str | None:
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
    counts = Counter(violation.fingerprint() for violation in violations)
    payload = {
        "version": BASELINE_VERSION,
        "gate": {"source": GATE_SOURCE, "sha256": gate_sha256()},
        "entries": dict(sorted(counts.items())),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return len(counts)


def split_by_baseline(
    violations: list[Violation], baseline: dict[str, int]
) -> tuple[list[Violation], list[str]]:
    covered = Counter()
    fresh: list[Violation] = []
    for violation in violations:
        key = violation.fingerprint()
        if covered[key] < baseline.get(key, 0):
            covered[key] += 1
        else:
            fresh.append(violation)
    stale = [key for key, need in baseline.items() if covered[key] < need]
    return fresh, stale


def default_roots() -> list[Path]:
    # .../crates/zhuge-ui-core/tools/<this file> — the crates directory is the root.
    crates = Path(__file__).resolve().parents[2]
    return [crates] if crates.is_dir() else []


def repo_root() -> Path:
    for candidate in Path(__file__).resolve().parents:
        if (candidate / ".git").exists():
            return candidate
    return Path.cwd()


def self_test() -> int:
    """Rule-level self-check: each fixture must trip exactly the law it names."""

    consumer = "taskmanager/crates/taskmanager-bevy-ui/src/layout.rs"
    outsider = "music-frog/crates/infiltrator-contract/src/palette.rs"
    cases: list[tuple[str, str, str, bool, tuple[str, ...]]] = [
        (
            consumer,
            "two identical scalar mixes in two functions count twice",
            "fn first(a: f32, b: f32, t: f32) -> f32 {\n    a + (b - a) * t\n}\n"
            "fn second(a: f32, b: f32, t: f32) -> f32 {\n    a + (b - a) * t\n}\n",
            True,
            ("UIK-MIX-001", "UIK-MIX-001"),
        ),
        (
            consumer,
            "a hand-spelled scalar mix is a second implementation",
            "fn width(base: f32, minimum: f32, factor: f32) -> f32 {\n    minimum + (base - minimum) * factor\n}\n",
            True,
            ("UIK-MIX-001",),
        ),
        (
            consumer,
            "the commuted spelling is the same fact",
            "fn alpha(min: f32, max: f32, wave: f32) -> f32 {\n    wave * (max - min) + min\n}\n",
            True,
            ("UIK-MIX-001",),
        ),
        (
            consumer,
            "the owner call is the legal form",
            "fn width(base: f32, minimum: f32, factor: f32) -> f32 {\n    lerp(minimum, base, factor)\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "the caller keeps its own progress policy",
            "fn slide(from: f32, to: f32, progress: f32) -> f32 {\n    lerp(from, to, progress.clamp(0.0, 1.0))\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a local wrapper that keeps its own progress policy is legal",
            "pub fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {\n    lerp(start, end, t.clamp(0.0, 1.0))\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "the same wrapper still cannot spell the arithmetic itself",
            "pub fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {\n    start + (end - start) * t.clamp(0.0, 1.0)\n}\n",
            True,
            ("UIK-MIX-001",),
        ),
        (
            consumer,
            "a per-channel color mix is that type's own face",
            "fn mix(start: Rgba, end: Rgba, t: f32) -> Rgba {\n    Rgba {\n        r: start.r + (end.r - start.r) * t,\n        g: start.g + (end.g - start.g) * t,\n    }\n}\n",
            True,
            (),
        ),
        (
            outsider,
            "a crate that does not depend on the owner has no owner to call",
            "fn opacity(min: f32, max: f32, breath: f32) -> f32 {\n    min + (max - min) * breath\n}\n",
            False,
            (),
        ),
        (
            "zhuge-ui-core/src/easing.rs",
            "the owner that defines the mix is exempt",
            "pub const fn lerp(from: f32, to: f32, progress: f32) -> f32 {\n    from + (to - from) * progress\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a product copy of the definition is not exempt",
            "pub const fn lerp(from: f32, to: f32, progress: f32) -> f32 {\n    from + (to - from) * progress\n}\n",
            True,
            ("UIK-MIX-001",),
        ),
        (
            consumer,
            "prose quoting the arithmetic is not an implementation",
            "// It used to be `minimum + (base - minimum) * factor`.\nfn layout() {}\n",
            True,
            (),
        ),
        (
            consumer,
            "an unrelated product is untouched",
            "fn area(width: f32, height: f32) -> f32 {\n    width * height / 2.0\n}\n",
            True,
            (),
        ),
    ]
    failures = 0
    for path, name, source, consumes, expected in cases:
        found = tuple(violation.code for violation in analyze(path, source, consumes))
        if found != expected:
            failures += 1
            print(
                f"self-test FAIL: {name}: expected {expected or 'no breach'}, "
                f"got {found or 'no breach'}",
                file=sys.stderr,
            )
    total = len(cases)
    must_not_report = sum(1 for case in cases if not case[-1])
    if must_not_report < 4:
        failures += 1
        print(
            f"self-test FAIL: only {must_not_report} must-not-report fixtures; "
            "a rule proved only by its positive cases is not proved at all",
            file=sys.stderr,
        )
    print(f"scalar mix guard: self-test {total - failures}/{total} fixtures behaved as specified; {must_not_report} must not report")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="scalar linear mix uniqueness guard")
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument("--root", type=Path, action="append", help="scan this file or directory")
    parser.add_argument("--baseline", type=Path, help="ratchet baseline file")
    parser.add_argument("--write-baseline", type=Path, help="record current debt as the baseline")
    parser.add_argument("--strict-provenance", action="store_true", help="accepted for compatibility; digest drift now always fails")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    repo = repo_root()
    roots = list(args.root) if args.root else default_roots()
    roots = [root if root.is_absolute() else repo / root for root in roots]
    if not roots:
        print("scalar mix guard: no scan root; pass --root <tree>", file=sys.stderr)
        return 2
    violations = scan(roots)

    if args.write_baseline:
        written = write_baseline(args.write_baseline, violations)
        print(f"scalar mix guard: baseline written with {written} entries -> {args.write_baseline}")
        return 0

    baseline: dict[str, int] = {}
    if args.baseline:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"scalar mix guard: baseline unreadable: {error}", file=sys.stderr)
            return 2
        mismatch = provenance_mismatch(args.baseline)
        if mismatch:
            message = (
                "scalar mix guard: ledger was written against a different gate "
                f"({mismatch}); refresh the vendored gate and the ledger together"
            )
            print(message, file=sys.stderr)
            return 1

    fresh, stale = split_by_baseline(violations, baseline)
    for violation in fresh:
        print(
            f"{violation.path}:{violation.line}: {violation.code}: {violation.detail}\n    {violation.snippet}",
            file=sys.stderr,
        )
    for key in stale:
        print(f"scalar mix guard: stale baseline entry: {key}", file=sys.stderr)

    scanned = sum(len(rust_files(root)) for root in roots)
    if scanned == 0:
        print("scalar mix guard: no Rust source scanned — refusing to pass silently", file=sys.stderr)
        return 1
    covered = len(violations) - len(fresh)
    print(
        f"scalar mix guard: scanned={scanned} violations={len(violations)} "
        f"new={len(fresh)} baselined={covered} stale={len(stale)}"
    )
    return 1 if fresh and args.mode == "enforce" else 0


if __name__ == "__main__":
    raise SystemExit(main())
