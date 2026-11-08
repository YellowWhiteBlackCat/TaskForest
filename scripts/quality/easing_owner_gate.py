#!/usr/bin/env python3
"""zhuge-ui-core 缓动斜坡与贝塞尔求解唯一性门禁：两个事实各只有一个 owner。

正文只有一份：``crates/zhuge-ui-core/README.md`` 的「Law — the ramp and the
Bézier solve each have one owner」。本文件是它的唯一机械执行器，随 crate 一起发布；
消费者从自己锁定的 crate 版本里取出同一份文件执行，规则实现只有一份，各产品只持有
自己的存量台账（棘轮基线）。

律法一 UIK-EASE-001（不得手写 smoothstep 斜坡）
    ``t * t * (3.0 - 2.0 * t)``（或展开式 ``3.0 * t * t - 2.0 * t * t * t``）就是
    ``zhuge_ui_core::easing::smoothstep``。在能调用 owner 的 crate 里再写一遍该算术，
    或本地另定义一个叫 ``smoothstep`` 的函数，即同一事实的第二实现。斜坡的**取反**
    （``1.0 - smoothstep(t)``）、输入钳制与退化答案是调用方的策略，留在原地。
    平方项改名不算逃逸：同一函数里同时出现 ``(3.0 - 2.0 * 名称)`` 因子与某个平方项也报。

律法二 UIK-EASE-002（不得手写 cubic-Bézier 缓动求解）
    「给定进度 x 解出曲线参数」是 ``easing::CubicBezier`` 的事实：一个函数里同时出现
    (i) 定次迭代、(ii) 用商收窄变量的步进（Newton 或二分中点）、(iii) 三次 Bernstein/
    Horner 多项式或对 cubic/bezier 求值 helper 的调用，即为第二权威。
    **只求值曲线而不解参数**（画一条线段、采样一段路径）是绘制事实，不在本律法之内。

判定的边界（写死，避免门把图形/颜色/着色器判成违规）：
    * **着色器源码不是 Rust**：``r#"..."#``/``r"..."`` 原始字符串整体掩掉，WGSL 内建
      ``smoothstep(a, b, x)`` 既不得触发也不得豁免；普通字符串与注释同样掩掉。
    * **未依赖 owner 的 crate 放行**：最近的 Cargo.toml 未声明 ``zhuge-ui-core`` 的层
      没有可调用的 owner，这是**依赖方向**事实，须由台账/TODO 写明该层是否应当依赖共享包。
    * **owner 自身放行**：core 内定义 ``smoothstep`` 的文件免 EASE-001，定义 ``solve_t``
      的文件免 EASE-002；消费者复制一份定义不获得豁免。

用法::

    python3 tools/check_easing_owner.py                       # 扫本仓库各 crate 的 src
    python3 tools/check_easing_owner.py --mode report --root <tree>
    python3 tools/check_easing_owner.py --write-baseline b.json
    python3 tools/check_easing_owner.py --baseline b.json
    python3 tools/check_easing_owner.py --self-test
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
GATE_SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_easing_owner.py"
OWNER_CRATE = "zhuge-ui-core"
# Matches a direct `zhuge-ui-core = ...` and the inherited `zhuge-ui-core.workspace = true` form.
OWNER_DEPENDENCY = re.compile(r"^\s*zhuge-ui-core(?:\.\w+)?\s*=", re.MULTILINE)
CRATE_NAME = re.compile(r'^name\s*=\s*"zhuge-ui-core"\s*$', re.MULTILINE)
RAMP_DEFINITION = re.compile(r"\bpub\s+(?:const\s+)?fn\s+smoothstep\b")
SOLVE_DEFINITION = re.compile(r"\bpub\s+fn\s+solve_t\b")

LINE_COMMENT = re.compile(r"//[^\n]*")
BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
RAW_STRING = re.compile(r"\b(?:b|r|br|rb)(#\S*?)?\"(?:\\.|[^\"\\])*?\"\1?", re.DOTALL)
STRING_LITERAL = re.compile(r"\"(?:[^\"\\\n]|\\.)*\"|'(?:[^'\\\n]|\\.)'")

# --- UIK-EASE-001: the smoothstep ramp -------------------------------------
THREE = r"(?:3(?:\.0*)?[a-z_0-9]*|THREE|THREE_F32)"
TWO = r"(?:2(?:\.0*)?[a-z_0-9]*|TWO|TWO_F32)"
RAMP_MULTIPLIED = re.compile(
    r"(?P<x>[A-Za-z_]\w*)\s*\*\s*(?P=x)\s*\*\s*\(\s*" + THREE + r"\s*-\s*" + TWO + r"\s*\*\s*(?P=x)\s*\)"
)
RAMP_EXPANDED = re.compile(
    THREE + r"\s*\*\s*(?P<x>[A-Za-z_]\w*)\s*\*\s*(?P=x)\s*-\s*" + TWO + r"\s*\*\s*(?P=x)"
    + r"|" + TWO + r"\s*\*\s*(?P<y>[A-Za-z_]\w*)\s*\*\s*(?P=y)\s*\*\s*(?P=y)\s*-\s*" + THREE + r"\s*\*\s*(?P=y)"
    + r"|-\s*" + TWO + r"\s*\*\s*(?P<z>[A-Za-z_]\w*)\s*\*\s*(?P=z)\s*\*\s*(?P=z)\s*\+\s*" + THREE + r"\s*\*\s*(?P=z)"
)
RAMP_FACTOR = re.compile(THREE + r"\s*-\s*" + TWO + r"\s*\*\s*[A-Za-z_(][\w.()]*")
RAMP_MACRO_BODY = re.compile(r"macro_rules!\s+\w*[a-z_]*ramp\w*|macro_rules!\s+\w*[a-z_]*smooth\w*", re.IGNORECASE)
OWNER_RAMP_CALL = re.compile(r"(?<!fn )\bsmoothstep\s*\(")
LOCAL_RAMP_DEFINITION = re.compile(r"\bfn\s+smoothstep\s*[(<]")

# --- UIK-EASE-002: the cubic-Bézier easing solve ----------------------------
SOLVER_LOOP = re.compile(r"\bfor\s+[\w_]+\s+in\s+0\s*\.\.\s*\d+")
NEWTON_STEP = re.compile(r"[-+]?=\s*[^=\n;]*/[^=\n;]*;")
BISECTION_MIDPOINT = re.compile(r"\(\s*[A-Za-z_]\w*\s*\+\s*[A-Za-z_]\w*\s*\)\s*(?:\*\s*0\.5|/\s*2\.0)")
BERNSTEIN_MIXED = re.compile(
    r"3\.0\s*\*\s*[A-Za-z_(][\w.()\[\]]*\s*\*\s*[A-Za-z_(][\w.()\[\]]*\s*\*\s*[A-Za-z_(]"
)
HORNER_CUBIC = re.compile(
    r"\(\s*\w+\s*\*\s*\w+\s*\+\s*\w+\s*\)\s*\*\s*\w+\s*\+\s*\w+\s*\)\s*\*\s*\w+"
)
BEZIER_EVAL_CALL = re.compile(r"\b\w*(?:cubic|bezier|bernstein)\w*\s*\(", re.IGNORECASE)
POLYNOMIAL_LOOP = re.compile(
    r"for\s+\w+\s+in\s+0\s*\.\.\s*4\s*\{[^}]*\+[^}]*powi"
    r"|powi\(\s*[\w\s()]+\s*\)",
    re.DOTALL,
)
CUBIC_COEFFICIENTS = re.compile(r"\[\s*f32\s*;\s*4\s*\]|powi\(\s*\w+\s*\*\s*3\s*\)")

SKIP_DIRECTORIES = {".git", "target", "node_modules", ".private", "dist", "build"}


@dataclass
class Violation:
    """One breach of an easing-ownership law, addressed by file, line and text."""

    path: str
    line: int
    code: str
    symbol: str
    detail: str
    snippet: str

    def fingerprint(self) -> str:
        """Baseline key: path, rule code, owning function and source text.

        The function is part of the key because two identical hand-spelled ramps in
        two functions are two copies of the fact; a key without it would let the
        second copy in for free.
        """

        return f"{self.path}|{self.code}|{self.symbol}|{self.snippet}"


def mask_noncode(text: str) -> str:
    """Blank comments, string literals and shader source while keeping lines."""

    def blank(match: re.Match[str]) -> str:
        return re.sub(r"[^\n]", " ", match.group(0))

    masked = BLOCK_COMMENT.sub(blank, text)
    masked = RAW_STRING.sub(blank, masked)
    masked = LINE_COMMENT.sub(blank, masked)
    return STRING_LITERAL.sub(blank, masked)


def normalize(line: str) -> str:
    text = " ".join(line.split())
    if len(text) > 160:
        return text[:157] + "..."
    return text


FN_HEADER = re.compile(r"\bfn\s+[A-Za-z_]\w*")
FN_NAME = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
TOP_LEVEL = "<file>"


def enclosing(lines: list[str], line: int) -> tuple[str, int]:
    """The function a breach line belongs to, and its header line.

    Brace counting is unusable here: these products embed the Bevy scene DSL, whose
    `bsn!` blocks carry interpolations and `--` separators that no Rust brace model
    balances. So the owner is the nearest preceding `fn` header that opens a body,
    and only when the breach is indented deeper than that header — a file-level
    `const` never inherits the previous function's name.
    """

    if line < 1 or line > len(lines):
        return TOP_LEVEL, 0
    breach = lines[line - 1]
    breach_indent = len(breach) - len(breach.lstrip())
    for index in range(line - 1, -1, -1):
        candidate = lines[index]
        if not FN_HEADER.search(candidate):
            continue
        indent = len(candidate) - len(candidate.lstrip())
        if breach_indent <= indent and index != line - 1:
            return TOP_LEVEL, 0
        name = FN_NAME.search(candidate)
        return (name.group(1) if name else TOP_LEVEL), index + 1
    return TOP_LEVEL, 0


def scopes(text: str) -> list[tuple[int, str]]:
    """Every ``fn`` body with its start line, nested functions included."""

    found: list[tuple[int, str]] = []
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if not FN_HEADER.search(line) or "{" not in line:
            continue
        depth = 0
        body: list[str] = []
        for offset in range(index, len(lines)):
            body.append(lines[offset])
            depth += lines[offset].count("{") - lines[offset].count("}")
            if depth <= 0 and offset > index:
                break
        found.append((index + 1, "\n".join(body)))
    return found


def ramp_breach(line: str) -> bool:
    """A smoothstep ramp re-derived on one line.

    The factor `3.0 - 2.0 * t` is the ramp's signature on its own, parenthesized
    or not: a square hoisted into a helper or renamed is the same fact. A corpus
    survey found no other product rule in either tree reading that factor.
    """

    if OWNER_RAMP_CALL.search(line):
        return False
    if RAMP_MACRO_BODY.search(line):
        return True
    return bool(RAMP_MULTIPLIED.search(line) or RAMP_EXPANDED.search(line) or RAMP_FACTOR.search(line))


def solve_breach(body: str, whole_file: str) -> bool:
    """A cubic-Bézier easing solve: iterate, narrow by a quotient, evaluate a cubic.

    The polynomial evaluator may live in a sibling function (`fn eval`), so an
    array/loop cubic is recognised from the whole file while the iteration that
    narrows a parameter must live in the reported function itself.
    """

    if not SOLVER_LOOP.search(body):
        return False
    narrowed = bool(NEWTON_STEP.search(body) or BISECTION_MIDPOINT.search(body))
    cubic = bool(
        BERNSTEIN_MIXED.search(body)
        or HORNER_CUBIC.search(body)
        or BEZIER_EVAL_CALL.search(body)
        or (
            POLYNOMIAL_LOOP.search(whole_file)
            and CUBIC_COEFFICIENTS.search(whole_file)
        )
    )
    return narrowed and cubic


def analyze(path: str, text: str, consumes_owner: bool) -> list[Violation]:
    """Every ramp or easing solve held a second time outside the owner."""

    masked = mask_noncode(text)
    exempt_ramp = OWNER_CRATE in path and bool(RAMP_DEFINITION.search(masked))
    exempt_solve = OWNER_CRATE in path and bool(SOLVE_DEFINITION.search(masked))
    if not consumes_owner:
        return []
    found: list[Violation] = []
    seen: set[tuple[str, str]] = set()
    lines = masked.splitlines()

    def record(line: int, code: str, detail: str, snippet: str) -> None:
        key = (enclosing(lines, line)[0], snippet)
        if key in seen:
            return
        seen.add(key)
        found.append(
            Violation(
                path=path,
                line=line,
                code=code,
                symbol=key[0],
                detail=detail,
                snippet=snippet,
            )
        )

    if not exempt_ramp:
        for number, line in enumerate(lines, start=1):
            if LOCAL_RAMP_DEFINITION.search(line):
                record(
                    number,
                    "UIK-EASE-001",
                    (
                        "a second smoothstep is defined locally; call "
                        "zhuge_ui_core::easing::smoothstep and keep the inversion, clamp and "
                        "degenerate answer at the call site"
                    ),
                    normalize(line),
                )
                continue
            if not ramp_breach(line):
                continue
            owner, header = enclosing(lines, number)
            if header and LOCAL_RAMP_DEFINITION.search(lines[header - 1]):
                continue  # already recorded as the local definition itself
            record(
                number,
                "UIK-EASE-001",
                (
                    "a smoothstep ramp is spelled by hand; call "
                    "zhuge_ui_core::easing::smoothstep and keep the inversion, clamp and "
                    "degenerate answer at the call site"
                ),
                normalize(line),
            )

    if not exempt_solve:
        for line, body in scopes(masked):
            if not solve_breach(body, masked):
                continue
            loop = SOLVER_LOOP.search(body)
            assert loop is not None
            offset = body[: loop.start()].count("\n")
            record(
                line + offset,
                "UIK-EASE-002",
                (
                    "a cubic-Bézier easing solve is iterated by hand; call "
                    "zhuge_ui_core::easing::CubicBezier — evaluating a curve is a different "
                    "fact, solving it for a parameter is not"
                ),
                normalize(body.splitlines()[offset]),
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

    consumer = "music-frog/crates/infiltrator-bevy-widgets/src/easing_probe.rs"
    outsider = "music-frog/crates/infiltrator-contract/src/easing_probe.rs"
    cases: list[tuple[str, str, str, bool, tuple[str, ...]]] = [
        (
            consumer,
            "a hand-spelled ramp is a second implementation",
            "fn coverage(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "the expanded polynomial is the same fact",
            "fn blend(t: f32) -> f32 {\n    3.0 * t * t - 2.0 * t * t * t\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "the inverted ramp is still the same ramp",
            "fn falloff(distance: f32, blur: f32) -> f32 {\n    let t = distance / blur;\n    (1.0 - t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "the reordered polynomial is the same ramp",
            "fn blend(t: f32) -> f32 {\n    -2.0 * t * t * t + 3.0 * t * t\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "renaming the square does not rename the fact",
            "fn edge(distance: f32, width: f32) -> f32 {\n    let t = distance / width;\n    let squared = t * t;\n    1.0 - squared * (3.0 - 2.0 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "a local function called smoothstep is the same fact",
            "fn smoothstep(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "the square hoisted into a helper is still the ramp",
            "fn shape(t: f32) -> f32 {\n    3.0 - 2.0 * t\n}\nfn ramp(t: f32, squared: f32) -> f32 {\n    squared * shape(t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "a Catmull-Rom spline is not the ramp",
            "fn control_points(previous: Point, start: Point, end: Point, next: Point) -> (Point, Point) {\n    let ctrl_a = Point::new(\n        start.x + (end.x - previous.x) * SMOOTH_TENSION,\n        start.y + (end.y - previous.y) * SMOOTH_TENSION,\n    );\n    let ctrl_b = Point::new(\n        end.x - (next.x - end.x) * SMOOTH_TENSION,\n        end.y - (next.y - end.y) * SMOOTH_TENSION,\n    );\n    (ctrl_a, ctrl_b)\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a test named after the ramp is not a second definition",
            "#[test]\nfn smoothstep_is_clamped_symmetric_and_finite() {\n    assert_eq!(smoothstep(0.5), 0.5);\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "type-suffixed coefficients are the same ramp",
            "fn ramp(t: f32) -> f32 {\n    t * t * (3.0_f32 - 2.0_f32 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "coefficients bound to named constants are the same ramp",
            "const THREE: f32 = 3.0;\nfn ramp(t: f32) -> f32 {\n    t * t * (THREE - 2.0 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "a macro body re-spelling the ramp is a second definition",
            "macro_rules! ramp { ($t:expr) => { $t * $t * (3.0 - 2.0 * $t) } }\nfn ramp_fn(t: f32) -> f32 { ramp!(t) }\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "a loop-evaluated cubic with a bisection solve is the same second authority",
            "fn solve(target: f32, c: [f32; 4]) -> f32 {\n    let mut lo = 0.0;\n    let mut hi = 1.0;\n    let mut mid = target;\n    for _ in 0..24 {\n        mid = (lo + hi) * 0.5;\n        if eval(c, mid) < target { lo = mid; } else { hi = mid; }\n    }\n    mid\n}\nfn eval(c: [f32; 4], t: f32) -> f32 {\n    let mut value = 0.0;\n    for i in 0..4 { value += c[i] * t.powi(i as i32); }\n    value\n}\n",
            True,
            ("UIK-EASE-002",),
        ),
        (
            consumer,
            "an unrelated layout coefficient is not the ramp",
            "fn inset(width: f32, gutter: f32) -> f32 {\n    3.0_f32 * width - 2.0_f32 * gutter\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a macro with no ramp name or ramp shape is not a second definition",
            "macro_rules! clamp_unit { ($v:expr) => { $v.clamp(0.0, 1.0) } }\nfn soft(t: f32) -> f32 { smoothstep(clamp_unit!(t)) }\n",
            True,
            (),
        ),
        (
            consumer,
            "a non-cubic loop with a bisection is not an easing solve",
            "fn root(f: fn(f32) -> f32) -> f32 {\n    let mut lo = 0.0;\n    let mut hi = 1.0;\n    for _ in 0..32 {\n        let mid = (lo + hi) * 0.5;\n        if f(mid) > 0.0 { hi = mid; } else { lo = mid; }\n    }\n    (lo + hi) * 0.5\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "the owner call with the caller's own inversion is legal",
            "fn falloff(t: f32) -> f32 {\n    (1.0 - smoothstep(t)).clamp(0.0, 1.0)\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "the owner call behind a local helper keeps one authority",
            "fn soft(t: f32) -> f32 {\n    let eased = zhuge_ui_core::easing::smoothstep(t);\n    eased * 0.8\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a shader's smoothstep builtin is not Rust code",
            'const WGSL: &str = r#"\n@fragment fn fs_main() -> @location(0) vec4<f32> {\n    let band = smoothstep(0.42, 0.5, phase);\n    let ramp = t * t * (3.0 - 2.0 * t);\n    return vec4<f32>(ramp, band, 0.0, 1.0);\n}\n"#;\nfn host() {}\n',
            True,
            (),
        ),
        (
            consumer,
            "a bytes raw string is masked whole",
            'const SHADER: &[u8] = br#"\\nlet ramp = t * t * (3.0 - 2.0 * t);\\n"#;\\nfn host() {}\\n',
            True,
            (),
        ),
        (
            consumer,
            "a double-hash raw string is masked whole",
            'const SHADER: &str = r##"\\nfn sdf(d: f32, w: f32) -> f32 {\\n    let t = ((d + w) / (2.0 * w)).clamp(0.0, 1.0);\\n    return 1.0 - t * t * (3.0 - 2.0 * t);\\n}\\n"##;\\nfn host() {}\\n',
            True,
            (),
        ),
        (
            consumer,
            "a second implementation in a test function is still a breach",
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn ramp_fixture() {\n        let t = 0.5;\n        assert_eq!(t * t * (3.0 - 2.0 * t), 0.5);\n    }\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "a shader in a ##-delimited raw string is not Rust code",
            'const SHADER: &str = r##"\nfn sdf_edge_coverage(d: f32, w: f32) -> f32 {\n    let t = clamp((d + w * 0.5) / w, 0.0, 1.0);\n    return 1.0 - t * t * (3.0 - 2.0 * t);\n}\n"##;\nfn host() {}\n',
            True,
            (),
        ),
        (
            consumer,
            "a Newton solve of a cubic-Bézier curve is a second authority",
            "fn evaluate(&self, progress: f32) -> f32 {\n    let mut parameter = target;\n    for _ in 0..8 {\n        let error = cubic_bezier(parameter, 0.0, self.x1, self.x2, 1.0) - target;\n        let slope = 3.0 * inverse * inverse * self.x1 + 3.0 * parameter * parameter * self.x2;\n        parameter = (parameter - error / slope).clamp(0.0, 1.0);\n    }\n    cubic_bezier(parameter, 0.0, self.y1, self.y2, 1.0)\n}\n",
            True,
            ("UIK-EASE-002",),
        ),
        (
            consumer,
            "a bisection solve of the same curve is the same second authority",
            "fn solve(target: f32, control: &[f32; 4]) -> f32 {\n    let mut low = 0.0;\n    let mut high = 1.0;\n    let mut middle = target;\n    for _ in 0..32 {\n        middle = (low + high) * 0.5;\n        if bezier_x(middle, control) < target {\n            low = middle;\n        } else {\n            high = middle;\n        }\n    }\n    middle\n}\n",
            True,
            ("UIK-EASE-002",),
        ),
        (
            consumer,
            "evaluating a curve is not solving it",
            "fn eval(&self, t: f32) -> PlotPoint {\n    let u = 1.0 - t;\n    let uu = u * u;\n    PlotPoint {\n        x: uu * u * self.p0.x + 3.0 * uu * t * self.c1.x,\n        y: uu * u * self.p0.y + 3.0 * uu * t * self.c1.y,\n    }\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "Newton on a colorimetric function is not a Bézier solve",
            "fn find_luminance(hue: f32, chroma: f32, y: f32) -> f32 {\n    let mut estimate = y;\n    for _ in 0..5 {\n        let error = measured_j(estimate) - y;\n        if error.abs() < 0.002 {\n            break;\n        }\n        estimate = estimate - error / slope(estimate);\n    }\n    estimate\n}\n",
            True,
            (),
        ),
        (
            consumer,
            "a bisection without a cubic is not a Bézier solve",
            "fn bisect_root(f: fn(f32) -> f32, low: f32, high: f32) -> f32 {\n    let mut lo = low;\n    let mut hi = high;\n    for _ in 0..40 {\n        let middle = (lo + hi) * 0.5;\n        if f(middle) > 0.0 {\n            hi = middle;\n        } else {\n            lo = middle;\n        }\n    }\n    (lo + hi) * 0.5\n}\n",
            True,
            (),
        ),
        (
            outsider,
            "a crate that does not depend on the owner has no owner to call",
            "fn coverage(t: f32) -> f32 {\n    t * t * (3.0 - 2.0 * t)\n}\n",
            False,
            (),
        ),
        (
            "zhuge-ui-core/src/easing.rs",
            "the owner that defines the ramp is exempt from the ramp rule",
            "pub fn smoothstep(progress: f32) -> f32 {\n    let t = progress.clamp(0.0, 1.0);\n    t * t * (3.0 - 2.0 * t)\n}\n",
            True,
            (),
        ),
        (
            "zhuge-ui-core/src/easing.rs",
            "the owner that defines the solve is exempt from the solve rule",
            "pub fn solve_t(&self, x: f32) -> f32 {\n    let mut t = x;\n    for _ in 0..8 {\n        let current_x = ((ax * t + bx) * t + cx) * t - x;\n        t -= current_x / ((3.0 * ax * t + 2.0 * bx) * t + cx);\n    }\n    t\n}\n",
            True,
            (),
        ),
        (
            "taskmanager/crates/taskmanager-gpui/src/easing_probe.rs",
            "a product copy of the ramp definition is not exempt",
            "pub fn smoothstep(progress: f32) -> f32 {\n    let t = progress.clamp(0.0, 1.0);\n    t * t * (3.0 - 2.0 * t)\n}\n",
            True,
            ("UIK-EASE-001",),
        ),
        (
            consumer,
            "prose quoting the ramp is not an implementation",
            "// The ramp used to read `t * t * (3.0 - 2.0 * t)` before the owner took it.\nfn layout() {}\n",
            True,
            (),
        ),
        (
            consumer,
            "an unrelated product rule is untouched",
            "fn inset(width: f32, gutter: f32) -> f32 {\n    (width - gutter * 2.0).max(0.0)\n}\n",
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
    print(f"easing owner guard: self-test {total - failures}/{total} fixtures behaved as specified; {must_not_report} must not report")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="easing ramp and Bézier-solve ownership guard")
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
        print("easing owner guard: no scan root; pass --root <tree>", file=sys.stderr)
        return 2
    violations = scan(roots)

    if args.write_baseline:
        written = write_baseline(args.write_baseline, violations)
        print(f"easing owner guard: baseline written with {written} entries -> {args.write_baseline}")
        return 0

    baseline: dict[str, int] = {}
    if args.baseline:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"easing owner guard: baseline unreadable: {error}", file=sys.stderr)
            return 2
        mismatch = provenance_mismatch(args.baseline)
        if mismatch:
            message = (
                "easing owner guard: ledger was written against a different gate "
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
        print(f"easing owner guard: stale baseline entry: {key}", file=sys.stderr)

    scanned = sum(len(rust_files(root)) for root in roots)
    if scanned == 0:
        print("easing owner guard: no Rust source scanned — refusing to pass silently", file=sys.stderr)
        return 1
    covered = len(violations) - len(fresh)
    print(
        f"easing owner guard: scanned={scanned} violations={len(violations)} "
        f"new={len(fresh)} baselined={covered} stale={len(stale)}"
    )
    return 1 if fresh and args.mode == "enforce" else 0


if __name__ == "__main__":
    raise SystemExit(main())
