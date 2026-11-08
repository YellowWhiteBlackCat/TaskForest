#!/usr/bin/env python3
"""zhuge-ui-core 带宽算术唯一性门禁：最宽带规则只有一个 owner。

正文只有一份：``crates/zhuge-ui-core/README.md`` 的「Law — 带宽算术只有一个 owner」。
本文件是它的唯一机械执行器，随 crate 一起发布；消费者从自己锁定的 crate 版本里
取出同一份文件执行，规则实现只有一份，各产品只持有自己的存量台账（棘轮基线）。

律法一 UIK-BAND-001（不得第二套带宽算术）
    一条语句同时出现「向上取整的行数」（``.ceil()`` / ``div_ceil(``）与 overscan /
    band 项，就是在重新推导“任意滚动偏移下最多挂载多少行”。必须调用
    ``zhuge_ui_core::virtual_list::max_band_rows``。

律法二 UIK-BAND-002（不得第二套池容量或挂载上界）
    给回收池规模或挂载上界命名的赋值/断言（``capacity`` / ``pool`` / ``slots`` /
    ``bound`` / ``max_visible`` 等）右侧含 ``.ceil()`` / ``div_ceil(``，同样必须调用
    owner。产品自己的**下限策略**（``.max(8)``）合法：它是策略，不是算术。

律法三 UIK-BAND-003（不得手工把带加宽两侧）
    出现 ``OVERSCAN * 2`` / ``2 * overscan`` / ``overscan.saturating_mul(2)`` 就是在写
    "带在两侧各扩一屏"的那一步。它常与取整式分处两个函数（一个返回可见带，另一个补两侧
    overscan），所以本条**不要求**同行出现 ``ceil``；仍必须经 ``max_band_rows`` 表达。

放行两条：任何调用 ``max_band_rows(`` 的语句；以及 owner 自身——同时满足「路径属于
``zhuge-ui-core``」与「本文件定义 ``pub fn max_band_rows``」，因此消费者复制一份定义
不获得豁免。注释与字符串在匹配前掩掉，说明文字不得触发也不得豁免。

用法::

    python3 tools/check_band_arithmetic.py                   # 扫本 crate 的 src
    python3 tools/check_band_arithmetic.py --mode report     # 只报告不失败
    python3 tools/check_band_arithmetic.py --root <tree>     # 消费者的 Rust 源码树
    python3 tools/check_band_arithmetic.py --write-baseline b.json
    python3 tools/check_band_arithmetic.py --baseline b.json
    python3 tools/check_band_arithmetic.py --self-test
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
GATE_SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_band_arithmetic.py"
OWNER_CRATE = "zhuge-ui-core"
OWNER_DEFINITION = re.compile(r"\bpub fn max_band_rows\b")

CEIL_SHAPE = re.compile(r"\.ceil\s*\(\s*\)|\bdiv_ceil\s*\(")
BAND_TERM = re.compile(r"\boverscan\b|\boverscan_[a-z0-9_]+|\b[A-Z0-9_]*OVERSCAN[A-Z0-9_]*\b|\bband\b", re.IGNORECASE)
POOL_NAME = re.compile(
    r"\b(?:slot_?capacity|capacity|pool_?size|pool|slot_?count|max_?mounted_?bound"
    r"|max_?visible|contract_?bound|mounted_?bound|bound|slots)\s*(?::|=)",
)
RULE_DEFINITION = re.compile(r"\bfn\s+max_band_rows\b")
OWNER_CALL = re.compile(r"\bmax_band_rows\s*\(")
BAND_DOUBLE = re.compile(
    r"\w*overscan\w*\s*\*\s*2\b|2\s*\*\s*\w*overscan\w*|\w*overscan\w*\.saturating_mul\(\s*2\s*\)",
    re.IGNORECASE,
)
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r"\"(?:[^\"\\\n]|\\.)*\"|'(?:[^'\\\n]|\\.)'")

SKIP_DIRECTORIES = {".git", "target", "node_modules", ".private", "dist", "build"}


@dataclass
class Violation:
    """One breach of the band-arithmetic law, addressed by file, line and text."""

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
    """Blank comments and string literals while keeping offsets and lines.

    Prose must neither satisfy nor trip a rule, and a doc comment that quotes
    the forbidden arithmetic is not a second implementation of it.
    """

    def blank(match: re.Match[str]) -> str:
        return " " * len(match.group(0)) if "\n" not in match.group(0) else match.group(0)

    masked = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.DOTALL)
    masked = LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), masked)
    return STRING_LITERAL.sub(blank, masked)


def statements(text: str) -> list[tuple[int, str]]:
    """Group source lines into bracket-balanced statements.

    The forbidden arithmetic is routinely written across several lines, so a
    per-line scan would miss it. A statement runs until brackets balance out,
    which keeps the match honest without a full Rust parser.
    """

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
        stripped = line
        depth += stripped.count("(") + stripped.count("[") + stripped.count("{")
        depth -= stripped.count(")") + stripped.count("]") + stripped.count("}")
        if depth <= 0:
            depth = 0
            grouped.append((start_line, "\n".join(buffer)))
            buffer = []
    if buffer:
        grouped.append((start_line, "\n".join(buffer)))
    return grouped


def normalize(line: str) -> str:
    return " ".join(line.split())


def matched_line(statement: str, *patterns: re.Pattern[str]) -> str:
    """The first statement line a pattern actually matched.

    A fingerprint must name the line that holds the arithmetic, so a ledger
    entry survives the surrounding function moving or re-wrapping.
    """

    lines = statement.splitlines()
    for pattern in patterns:
        for raw in lines:
            if pattern.search(mask_noncode(raw)):
                return raw
    return lines[0] if lines else ""


def analyze(path: str, text: str) -> list[Violation]:
    """Every band or pool arithmetic re-derived outside the rule's owner."""

    if OWNER_CRATE in path and OWNER_DEFINITION.search(mask_noncode(text)):
        return []
    masked = mask_noncode(text)
    found: list[Violation] = []
    lines = masked.splitlines()
    for line, statement in statements(masked):
        symbol = enclosing(lines, line)
        calls_owner = OWNER_CALL.search(statement) and not RULE_DEFINITION.search(statement)
        # The two-sided widening is a breach on its own, and it often lives in a
        # different function from the rounding, so this rule asks for no `ceil`.
        # One statement reports once: a full re-derivation (001) already names
        # the widening step, so 003 is only for the hand-widening that stands alone.
        re_derives_band = CEIL_SHAPE.search(statement) and BAND_TERM.search(statement)
        if BAND_DOUBLE.search(statement) and not calls_owner and not re_derives_band:
            found.append(
                Violation(
                    path=path,
                    line=line,
                    symbol=symbol,
                    code="UIK-BAND-003",
                    detail=(
                        "the overscan band is widened by hand; call "
                        "zhuge_ui_core::virtual_list::max_band_rows, which owns the two-sided widening"
                    ),
                    snippet=normalize(matched_line(statement, BAND_DOUBLE)),
                )
            )
        if calls_owner or not CEIL_SHAPE.search(statement):
            continue
        if BAND_TERM.search(statement):
            found.append(
                Violation(
                    path=path,
                    line=line,
                    symbol=symbol,
                    code="UIK-BAND-001",
                    detail=(
                        "second copy of the worst-case band arithmetic; call "
                        "zhuge_ui_core::virtual_list::max_band_rows"
                    ),
                    snippet=normalize(matched_line(statement, CEIL_SHAPE)),
                )
            )
        elif POOL_NAME.search(statement):
            found.append(
                Violation(
                    path=path,
                    line=line,
                    symbol=symbol,
                    code="UIK-BAND-002",
                    detail=(
                        "a pool size or mount bound derived by hand; call "
                        "zhuge_ui_core::virtual_list::max_band_rows (a minimum pool size is "
                        "policy, this is arithmetic)"
                    ),
                    snippet=normalize(matched_line(statement, CEIL_SHAPE, POOL_NAME)),
                )
            )
    return found


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
            violations.extend(analyze(f"{label}/{relative}", text))
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
    crate_src = Path(__file__).resolve().parent.parent / "src"
    return [crate_src] if crate_src.is_dir() else []


def repo_root() -> Path:
    for candidate in Path(__file__).resolve().parents:
        if (candidate / ".git").exists():
            return candidate
    return Path.cwd()


def self_test() -> int:
    """Rule-level self-check: each fixture must trip exactly the law it names."""

    consumer = "taskmanager/crates/taskmanager-iced/src/page.rs"
    cases: list[tuple[str, str, str, tuple[str, ...]]] = [
        (
            consumer,
            "two identical band derivations in two functions count twice",
            "fn first(v: f32, row: f32) -> usize {\n    ((v / row).ceil() as usize) + OVERSCAN * 2\n}\n"
            "fn second(v: f32, row: f32) -> usize {\n    ((v / row).ceil() as usize) + OVERSCAN * 2\n}\n",
            ("UIK-BAND-001", "UIK-BAND-001"),
        ),
        (
            consumer,
            "capacity beside an overscan term is a second band arithmetic",
            "fn page() {\n    let capacity = ((viewport / row).ceil() as usize)\n        .saturating_add(OVERSCAN * 2);\n}\n",
            ("UIK-BAND-001",),
        ),
        (
            consumer,
            "a mount bound named beside ceil is a second bound",
            "fn proof() {\n    let bound = (viewport / row).ceil() as usize + 2;\n}\n",
            ("UIK-BAND-002",),
        ),
        (
            consumer,
            "a band widened by hand in its own function is a second owner",
            "fn bound(viewport: f32) -> usize {\n    visible_rows(viewport) + RULE_WINDOW_OVERSCAN * 2\n}\n",
            ("UIK-BAND-003",),
        ),
        (
            consumer,
            "the same widening written as two times the band is caught",
            "fn pool() {\n    let slots = band + 2 * overscan;\n}\n",
            ("UIK-BAND-003",),
        ),
        (
            consumer,
            "the owner call owns the two-sided widening too",
            "fn bound(viewport: f32) -> usize {\n    max_band_rows(ROW_HEIGHT, viewport, RULE_WINDOW_OVERSCAN)\n}\n",
            (),
        ),
        (
            consumer,
            "the owner call is the legal form",
            "fn page() {\n    let capacity = max_band_rows(row, viewport, OVERSCAN).max(8);\n}\n",
            (),
        ),
        (
            consumer,
            "unrelated ceiling arithmetic stays untouched",
            "fn tick(dt: f32) {\n    let steps = (dt / MAX_STEP_SECS).ceil().max(1.0) as usize;\n}\n",
            (),
        ),
        (
            consumer,
            "prose quoting the arithmetic is not an implementation",
            "// The old form was (viewport / row).ceil() + OVERSCAN * 2.\nfn page() {}\n",
            (),
        ),
        (
            "zhuge-ui-core/src/virtual_list.rs",
            "the owner that defines the rule is exempt",
            "pub fn max_band_rows(a: f32, b: f32, c: usize) -> usize {\n    (b / a).ceil() as usize + c\n}\n",
            (),
        ),
        (
            "taskmanager/crates/ui/src/virtual_list.rs",
            "a product copy of the definition is not exempt",
            "pub fn max_band_rows(a: f32, b: f32, c: usize) -> usize {\n    (b / a).ceil() as usize + c * OVERSCAN\n}\n",
            ("UIK-BAND-001",),
        ),
    ]
    failures = 0
    for path, name, source, expected in cases:
        found = tuple(violation.code for violation in analyze(path, source))
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
    print(f"band arithmetic guard: self-test {total - failures}/{total} fixtures behaved as specified; {must_not_report} must not report")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="band arithmetic uniqueness guard")
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
        print("band arithmetic guard: no scan root; pass --root <tree>", file=sys.stderr)
        return 2
    violations = scan(roots)

    if args.write_baseline:
        written = write_baseline(args.write_baseline, violations)
        print(f"band arithmetic guard: baseline written with {written} entries -> {args.write_baseline}")
        return 0

    baseline: dict[str, int] = {}
    if args.baseline:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"band arithmetic guard: baseline unreadable: {error}", file=sys.stderr)
            return 2
        mismatch = provenance_mismatch(args.baseline)
        if mismatch:
            message = (
                "band arithmetic guard: ledger was written against a different gate "
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
        print(f"band arithmetic guard: stale baseline entry: {key}", file=sys.stderr)

    scanned = sum(len(rust_files(root)) for root in roots)
    if scanned == 0:
        print("band arithmetic guard: no Rust source scanned — refusing to pass silently", file=sys.stderr)
        return 1
    covered = len(violations) - len(fresh)
    print(
        f"band arithmetic guard: scanned={scanned} violations={len(violations)} "
        f"new={len(fresh)} baselined={covered} stale={len(stale)}"
    )
    return 1 if fresh and args.mode == "enforce" else 0


if __name__ == "__main__":
    raise SystemExit(main())
