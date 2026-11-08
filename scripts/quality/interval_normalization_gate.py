#!/usr/bin/env python3
"""zhuge-ui-core 区间归一化唯一性门禁：进入单位区间的投影只有一个 owner。

正文只有一份：``crates/zhuge-ui-core/README.md`` 的「Law — the interval projection has one
owner」。本文件是它的唯一机械执行器，随 crate 一起发布；消费者从自己锁定的 crate
版本里取出同一份文件执行，规则实现只有一份，各产品只持有自己的存量台账（棘轮基线）。

律法一 UIK-VALUE-001（不得手写区间投影）
    同一条语句里同时出现「减去一个下界」「除以一个区间长度」「把商夹进 0..=1」，就是在
    重新实现 ``zhuge_ui_core::value::interval_fraction``。必须调用 owner，并把退化答案写成
    显式的 `flat`，不得由一次除法偶然决定。

律法二 UIK-VALUE-002（不得把同一投影拆成几句写）
    同一个函数里这三步分处不同语句——先 ``let span = ceiling - floor;``，再除它，最后夹商
    （或把两端答案写成 ``if value <= min {0.0} else if value >= max {1.0}`` 分支）——与 001
    是同一个事实的第二权威。UIK-BAND-003 的教训直接写在这里：只读单语句的扫描会漏掉拆分，
    所以本条在**作用域**内把三步合起来看，命中时报告那条做除法的语句。

律法四 UIK-VALUE-004（本地小助手不是第二个 owner）
    把「减下界 / 除长度」包进 `fn project(...)`，再由调用方 `.clamp(0.0, 1.0)`，仍是同一条投影：
    助手里的商 + 别处的单位钳，合起来看就是第二权威。助手只调用 `interval_fraction` 才合法，
    保留自己的 `flat` 策略不受影响。**不报**只做槽位预算的本地商（`usable_w / (points - 1)`），
    也不报根本不做投影的助手（必须不报的 fixture 钉住这两类）。

律法三 UIK-VALUE-003（换个名字不改变事实）
    分母不叫 span 也一样：只要它是**由差式绑定出来的**（``let reach = ceiling - floor;``），
    且被除数减的是区间下界，就是同一条投影穿了另一件衣服。倒数写法（``1.0 / span`` 再乘回）、
    ``saturating_sub`` 的下界减法、以及 ``clamp(0.0_f32, 1.0_f32)`` / ``clamp(0., 1.)`` /
    ``.max(0.0).min(1.0)`` 这些字面量变体，都在同一判据内。

放行三种**不同事实**（各有 fixture 钉住"必须不报"）：没有单位钳的商（沿段比例、
``usable_w / (points - 1)`` 这类槽位预算）、像素覆盖斜坡（``1.0 - (row - top) / span``，量的是
光栅行而非模型值）、以及 Newton 修正里除以导数的 ``x - error / slope``。词汇表只收数值区间
的名字（span / range / extent / spread / travel / denom），不写 width / height，免得把布局几何
判成违规。注释与字符串在匹配前掩掉：说明文字既不得触发也不得豁免。

用法::

    python3 tools/check_interval_normalization.py                        # 扫本 crate 的 src
    python3 tools/check_interval_normalization.py --mode report --root <tree>
    python3 tools/check_interval_normalization.py --write-baseline b.json
    python3 tools/check_interval_normalization.py --baseline b.json
    python3 tools/check_interval_normalization.py --self-test            # 31 个 fixture
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
GATE_SOURCE = "zhuge-ui-kit:crates/zhuge-ui-core/tools/check_interval_normalization.py"
OWNER_CRATE = "zhuge-ui-core"
OWNER_DEFINITION = re.compile(r"\bpub fn interval_fraction\b")
OWNER_CALL = re.compile(r"(?<!fn )interval_fraction\s*\(")
LOCAL_FUNCTION = re.compile(r"\bfn\s+([A-Za-z_]\w*)\s*\(")

SPAN_WORDS = ("span", "range", "extent", "spread", "travel", "denom", "denominator")
LOWER_WORDS = (
    "min",
    "minimum",
    "lo",
    "low",
    "floor",
    "baseline",
    "base",
    "lower",
    "bottom",
    "from",
    "origin_value",
)
DIVISOR = re.compile(r"/\s*([A-Za-z_][\w.]*(?:\.\w+)*)|\[\s*\w+\s*-\s*\w+\s*\]\s*/")
DIVIDED_BY_DIFFERENCE = re.compile(r"/\s*\(([^()]*)\)", re.DOTALL)
RECIPROCAL_MULTIPLIED = re.compile(
    r"[*]\s*[A-Za-z_][\w.]*(?:inv|inverse|reciprocal)[\w.]*|[A-Za-z_][\w.]*inv\w*\s*[*]",
    re.IGNORECASE,
)
SUBTRACTION = re.compile(r"(?<![=!<>-])[-]\s*(?![=])")
# A unit clamp is a clamp to the two ends of the axis, however the literals are
# spelled: `0.0`, `0.`, `0.0_f32`, `0f64` and `0` all say the same thing, and
# min/max written in either order is the same clamp.
_UNIT_END = r"0(?:\.[0-9]*)?(?:[a-z_0-9]+)?|1(?:\.[0-9]*)?(?:[a-z_0-9]+)?"
UNIT_CLAMP = re.compile(
    r"\.clamp\(\s*(?:" + _UNIT_END + r")\s*,\s*(?:" + _UNIT_END + r")\s*\)"
    r"|\.min\(\s*1(?:\.[0-9]*)?(?:[a-z_0-9]+)?\s*\)\s*\.max\(\s*0(?:\.[0-9]*)?(?:[a-z_0-9]+)?\s*\)"
    r"|\.max\(\s*0(?:\.[0-9]*)?(?:[a-z_0-9]+)?\s*\)\s*\.min\(\s*1(?:\.[0-9]*)?(?:[a-z_0-9]+)?\s*\)"
)
# The same answer written as branches: guard the ends of the declared range and
# hand back the literal ends of the axis, with the travel in between divided out.
UPPER_BOUND = re.compile(
    r">=\s*[\w.]*(?:max|maximum|hi|high|ceiling|upper|top_value|peak)[\w.]*\b",
    re.IGNORECASE,
)
LOWER_GUARD = re.compile(
    r"<=\s*[\w.]*(?:min|minimum|lo|low|floor|baseline|base|lower|bottom)[\w.]*\b",
    re.IGNORECASE,
)
ARROW = re.compile(r"\bfn\s+[\w][\w!]*")
LINE_COMMENT = re.compile(r"//[^\n]*")
STRING_LITERAL = re.compile(r"\"(?:[^\"\\\n]|\\.)*\"|'(?:[^'\\\n]|\\.)'")

SKIP_DIRECTORIES = {".git", "target", "node_modules", ".private", "dist", "build"}


@dataclass
class Violation:
    """One breach of the interval-projection law, addressed by file, line and text."""

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

    Prose must neither satisfy nor trip a rule, and a doc comment that quotes the
    forbidden arithmetic is not a second implementation of it.
    """

    def blank(match: re.Match[str]) -> str:
        return " " * len(match.group(0)) if "\n" not in match.group(0) else match.group(0)

    masked = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.DOTALL)
    masked = LINE_COMMENT.sub(lambda m: " " * len(m.group(0)), masked)
    return STRING_LITERAL.sub(blank, masked)


def statements(text: str, base: int = 0) -> list[tuple[int, str]]:
    """Group source into bracket-balanced statements at one nesting level.

    A statement runs until the brackets are back at ``base`` and the line ends the
    statement (``;``, or ``}`` for a block expression). Splitting at the closing
    brace alone would swallow a whole function body, so callers pass the body's
    inner text and get its statements back. The forbidden projection is routinely
    written across several lines, so a per-line scan would miss it.
    """

    grouped: list[tuple[int, str]] = []
    depth = base
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
        depth = max(depth, base)
        ended = line.rstrip().endswith((";", "}", "{"))
        if depth == base and ended:
            grouped.append((start_line, "\n".join(buffer)))
            buffer = []
    if buffer:
        grouped.append((start_line, "\n".join(buffer)))
    return grouped


def fn_bodies(text: str) -> list[tuple[int, str]]:
    """Yield ``(body_first_line, inner_text)`` for every ``fn`` in the source.

    ``inner_text`` keeps the file's line numbering by padding the lines before the
    body with blanks, so a violation found inside reports the line the reader sees.
    """

    bodies: list[tuple[int, str]] = []
    lines = text.splitlines()
    head = re.compile(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+)*fn\s+[\w]+")
    index = 0
    while index < len(lines):
        if head.match(lines[index]):
            open_index = None
            cursor = index
            while cursor < len(lines):
                if "{" in lines[cursor]:
                    open_index = cursor
                    break
                if ";" in lines[cursor]:  # a trait or fn-pointer declaration, no body
                    break
                cursor += 1
            if open_index is None:
                index += 1
                continue
            depth = 0
            end_index = open_index
            for candidate in range(open_index, len(lines)):
                line = lines[candidate]
                depth += line.count("{")
                depth -= line.count("}")
                if depth <= 0:
                    end_index = candidate
                    break
            inner_lines = lines[open_index + 1 : end_index]
            padded = [""] * (open_index + 1) + inner_lines
            bodies.append((open_index + 2, "\n".join(padded)))
            index = end_index + 1
            continue
        index += 1
    return bodies


def normalize(line: str) -> str:
    return " ".join(line.split())


def matched_line(statement: str, *patterns: re.Pattern[str]) -> str:
    """The first statement line a pattern actually matched.

    A fingerprint must name the line that holds the arithmetic, so a ledger entry
    survives the surrounding function moving or re-wrapping.
    """

    lines = statement.splitlines()
    for pattern in patterns:
        for raw in lines:
            if pattern.search(raw):
                return raw
    return lines[0] if lines else ""


def divides_by_span(statement: str) -> bool:
    """Whether the statement divides by a numeric range, or multiplies by its reciprocal.

    ``x * inverse`` with ``let inverse = 1.0 / span`` is the same division wearing a
    multiplication, and a gate that only reads ``/`` would be blind to it.
    """

    if RECIPROCAL_MULTIPLIED.search(statement):
        return True
    for match in DIVISOR.finditer(statement):
        token = match.group(1)
        if token is None:
            return True
        last = token.split(".")[-1].lower()
        if any(word in last for word in SPAN_WORDS):
            return True
    for match in DIVIDED_BY_DIFFERENCE.finditer(statement):
        inner = match.group(1)
        if SUBTRACTION.search(inner):
            return True
    return False


def subtracts_lower_bound(statement: str) -> bool:
    """Whether the statement subtracts an operand that names the range's lower end."""

    operands = [
        match.group(0).split(".")[-1].lower()
        for match in re.finditer(
            r"(?:" + r"|".join(re.escape(word) for word in LOWER_WORDS) + r")[\w]*", statement, re.IGNORECASE
        )
    ]
    for match in SUBTRACTION.finditer(statement):
        operand = re.match(r"\s*([\w.]+)", statement[match.end() :])
        if operand and operand.group(1).split(".")[-1].lower() in operands:
            return True
    # `value.saturating_sub(min)` is the same clamp-free step toward the start.
    for match in re.finditer(r"\.saturating_sub\(\s*([\w.]+)", statement):
        if match.group(1).split(".")[-1].lower() in operands:
            return True
    return False


def span_bindings(scope: str) -> dict[str, set[str]]:
    """Map every ``let name = A - B;`` binding to the operands of its difference.

    The rule reads the binding because a hand-rolled projection usually names its
    denominator once and divides by it later, which is exactly the spelling a
    single-statement scan cannot see.
    """

    bindings: dict[str, set[str]] = {}
    pattern = re.compile(r"\blet\s+(?:mut\s+)?([a-z_][\w]*)[^=;]*=\s*([^;]+);")
    for match in pattern.finditer(scope):
        name, expression = match.group(1), match.group(2)
        operands: set[str] = set()
        for difference in SUBTRACTION.finditer(expression):
            tail = re.match(r"([A-Za-z_][\w.]*)", expression[difference.end() :])
            if tail is None:
                continue
            operands.add(tail.group(1).split(".")[-1])
            left = re.findall(r"([A-Za-z_][\w.]*)\s*$", expression[: difference.start()])
            if left:
                operands.add(left[0].split(".")[-1])
        if len(operands) >= 2:
            bindings[name] = operands
    return bindings


def unwrap(text: str) -> str:
    """Strip parentheses that wrap the whole expression, once at a time."""

    text = text.strip()
    while text.startswith("(") and text.endswith(")"):
        depth = 0
        closes_at_end = False
        for index, char in enumerate(text):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    closes_at_end = index == len(text) - 1
                    break
        if not closes_at_end:
            break
        text = text[1:-1].strip()
    return text


def top_level_difference(text: str) -> str | None:
    """The operand subtracted at bracket depth zero, if the expression has one.

    ``row - floor`` and ``(row - floor) as f32`` both answer ``floor``; ``a - b * c``
    answers ``b``; a product or a call with no subtraction answers nothing. A
    projection's first step is exactly this subtraction, so reading it at depth
    zero is what separates "measured into a range" from "anything with a minus".
    """

    unwrapped = unwrap(text)
    depth = 0
    for index, char in enumerate(unwrapped):
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "-" and depth == 0 and index > 0:
            if unwrapped[index - 1] == "-" or unwrapped[index + 1 : index + 2] == ">":
                continue
            operand = re.match(r"\s*([A-Za-z_][\w.]*(?:\s*\.\s*\w+\([^()]*\))?)", unwrapped[index + 1 :])
            if operand:
                return operand.group(1).split(".")[-1].split("(")[0]
    return None


def operand_before_division(statement: str, slash: int) -> str:
    """The dividend: the expression immediately to the left of this ``/``."""

    end = slash
    while end > 0 and statement[end - 1].isspace():
        end -= 1
    start = end
    depth = 0
    while start > 0:
        char = statement[start - 1]
        if char in ")]}":
            depth += 1
        elif char in "([{":
            if depth == 0:
                break
            depth -= 1
        elif depth == 0 and char in ",;=( &[|+-":
            if char == "-" and statement[start - 2 : start - 1] in ("", " "):
                break
            if char == "-" and start < end and statement[start:end].strip() and not statement[start:end].strip()[0].isalpha():
                break
            if char in ",;=( &[|":
                break
            if char == "-" and statement[start:end].strip().endswith("f32"):
                continue
        start -= 1
    return statement[start:end]


def is_lower_bound(name: str) -> bool:
    """Whether an operand names the lower end of a declared numeric range."""

    last = name.split(".")[-1].lower()
    return any(
        last == word or last.endswith("_" + word) or last.startswith(word)
        for word in LOWER_WORDS
        if len(word) > 1 or last == word
    )


def dividend_projects_range(statement: str) -> bool:
    """Whether what sits left of a division is the step into the range.

    The cross-statement rule needs more than a division by a span: the dividend
    must subtract the range's lower end (`value - min`), or be the unit literal of
    a reciprocal (`1.0 / span`, which is the same division wearing an
    multiplication). A width divided by a point count is a slot budget, and neither
    of those is true of it.
    """

    for match in DIVISOR.finditer(statement):
        dividend = operand_before_division(statement, match.start())
        subtracted = top_level_difference(dividend)
        if subtracted is not None and is_lower_bound(subtracted):
            return True
        saturating = re.search(r"\.saturating_sub\(\s*([\w.]+)", dividend)
        if saturating and is_lower_bound(saturating.group(1)):
            return True
        if re.fullmatch(r"\s*1(?:\.0*)?[a-z_0-9]*\s*", dividend):
            return True
    return False


def divides_by_value_span(statement: str, bindings: dict[str, set[str]]) -> str | None:
    """The bound denominator of a hand-written value-axis projection.

    Three things must hold, and each one is a step of the projection itself: the
    dividend subtracts a **value range's lower end** (``min``, ``floor``,
    ``baseline`` …), the divisor names a binding whose own value is a difference,
    and the quotient is clamped to the unit interval by the caller. That is the
    ``span = max - min`` shape wearing another name — the hole UIK-BAND-003 proved
    a name-only scan leaves open.

    A **paint coverage ramp** is deliberately outside this rule. ``1.0 - (row -
    top) / span`` measures a raster row into a band of pixels and inverts it into
    alpha, which is the surface's own axis, not a projection of a model value; its
    divisor is a one-pixel floor that keeps the wash from dividing by nothing. Both
    products spell it that way (TaskForest ``chart/canvas.rs``, MusicFrog
    ``chart.rs``), and a fixture pins the distinction so the gate cannot drift into
    the difference.
    """

    for match in DIVISOR.finditer(statement):
        token = match.group(1)
        if token is None:
            continue
        name = token.split(".")[-1]
        operands = bindings.get(name)
        if not operands:
            continue
        subtracted = top_level_difference(operand_before_division(statement, match.start()))
        if subtracted is None:
            continue
        if any(
            last == word or last.endswith("_" + word) or last.startswith(word + "_")
            for word in LOWER_WORDS
            for last in [subtracted.lower()]
        ):
            return name
    return None


def projection_steps(scope: str) -> tuple[bool, bool, bool]:
    """Whether the three steps of the projection are spelled inside one scope."""

    divides = subtracts = clamps = False
    for _, statement in statements(scope):
        if OWNER_CALL.search(statement):
            continue
        divides = divides or divides_by_span(statement)
        subtracts = subtracts or subtracts_lower_bound(statement)
        clamps = clamps or bool(UNIT_CLAMP.search(statement))
    return divides, subtracts, clamps


def analyze(path: str, text: str) -> list[Violation]:
    """Every interval projection re-derived outside the rule's owner."""

    if OWNER_CRATE in path and OWNER_DEFINITION.search(mask_noncode(text)):
        return []
    masked = mask_noncode(text)
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

    def scan_scope(scope: str) -> None:
        divides, subtracts, clamps = projection_steps(scope)
        bindings = span_bindings(scope)
        reported_here: set[int] = set()
        guarded = bool(LOWER_GUARD.search(scope)) and bool(UPPER_BOUND.search(scope))
        if divides and subtracts and (clamps or guarded):
            for line, statement in statements(scope):
                if OWNER_CALL.search(statement) or not divides_by_span(statement):
                    continue
                # A quotient that never measures into the range is not this law:
                # `usable_w / (points - 1)` is a slot budget, and the report needs
                # the ratio step itself, not merely a division inside the scope.
                if not subtracts_lower_bound(statement) and not dividend_projects_range(statement):
                    continue
                spelled_here = subtracts_lower_bound(statement) and bool(
                    UNIT_CLAMP.search(statement)
                )
                code = "UIK-VALUE-001" if spelled_here else "UIK-VALUE-002"
                detail = (
                    (
                        "a range is normalized into 0..=1 by hand; call "
                        "zhuge_ui_core::value::interval_fraction and pass the degenerate answer as `flat`"
                    )
                    if code == "UIK-VALUE-001"
                    else (
                        "the lower bound, the span and the unit clamp are spelled in separate statements "
                        "of one scope, which is the same second authority; call "
                        "zhuge_ui_core::value::interval_fraction"
                    )
                )
                reported_here.add(line)
                record(
                    line,
                    code,
                    detail,
                    normalize(matched_line(statement, DIVISOR, DIVIDED_BY_DIFFERENCE, SUBTRACTION, UNIT_CLAMP)),
                )
        # A denominator renamed away from the vocabulary is the same fact, so this
        # rule reads the binding instead of the name: `let reach = bottom - top;`
        # then `(row - top) / reach` clamped to the unit interval is UIK-VALUE-003.
        if not bindings or not clamps:
            return
        for line, statement in statements(scope):
            if line in reported_here or OWNER_CALL.search(statement):
                continue
            name = divides_by_value_span(statement, bindings)
            if name is None:
                continue
            record(
                line,
                "UIK-VALUE-003",
                (
                    "the denominator is a difference bound under another name and the numerator "
                    "subtracts one of its own operands, which is the interval projection renamed; "
                    "call zhuge_ui_core::value::interval_fraction"
                ),
                normalize(matched_line(statement, DIVISOR, DIVIDED_BY_DIFFERENCE, SUBTRACTION, UNIT_CLAMP)),
            )

    for _, body in fn_bodies(masked):
        scan_scope(body)
    # A projection spelled outside any `fn` — a module-level constant or a static
    # initializer — is the same breach, and the file-level pass skips statements
    # that open a body because their own `fn` pass already reported them.
    for line, statement in statements(masked):
        if re.search(r"\bfn\s", statement):
            continue
        scan_scope(statement)

    # UIK-VALUE-004: a local helper holds the quotient while another function
    # clamps its result into the unit interval. The three steps are then spread
    # across a caller/callee pair, which the single-scope rules cannot see.
    source_lines = masked.splitlines()
    quotients: list[tuple[str, int, str]] = []
    for body_line, body in fn_bodies(masked):
        header = ""
        for raw in reversed(source_lines[: max(body_line - 1, 0)]):
            if LOCAL_FUNCTION.search(raw):
                header = raw
                break
        name = LOCAL_FUNCTION.search(header)
        if not name or OWNER_CALL.search(body):
            continue
        for line, statement in statements(body):
            if not divides_by_span(statement):
                continue
            if not (subtracts_lower_bound(statement) or dividend_projects_range(statement)):
                continue
            quotients.append(
                (
                    name.group(1),
                    line,
                    normalize(matched_line(statement, DIVISOR, SUBTRACTION)),
                )
            )
    for helper_name, line, snippet in quotients:
        caller_pattern = re.compile(
            r"\b" + re.escape(helper_name) + r"\s*\([^)]*\)\s*\.\s*clamp"
        )
        if not caller_pattern.search(masked):
            continue
        record(
            line,
            "UIK-VALUE-004",
            (
                "a local helper re-derives the interval quotient and its caller supplies "
                "the unit clamp; the helper must call "
                "zhuge_ui_core::value::interval_fraction, keeping its own flat policy"
            ),
            snippet,
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

    consumer = "taskmanager/crates/taskmanager-bevy-ui/src/widgets/chart.rs"
    cases: list[tuple[str, str, str, tuple[str, ...]]] = [
        (
            consumer,
            "a local helper's quotient plus a caller's unit clamp is the same projection",
            "fn project(value: f32, floor: f32, ceiling: f32) -> f32 {\n    (value - floor) / (ceiling - floor)\n}\n"
            "fn surface(v: f32) -> f32 {\n    project(v, 0.0, 10.0).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-004",),
        ),
        (
            consumer,
            "a slot-budget helper is not an interval projection",
            "fn budget(usable_w: f32, points: usize) -> f32 {\n    usable_w / (points - 1) as f32\n}\n"
            "fn surface() -> f32 {\n    budget(100.0, 5).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "a helper that calls the owner keeps one authority",
            "fn project(value: f32, floor: f32, ceiling: f32, flat: f32) -> f32 {\n"
            "    interval_fraction(value, floor, ceiling, flat)\n}\n"
            "fn surface(v: f32) -> f32 {\n    project(v, 0.0, 10.0, 0.0).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "two identical projections in two functions count twice",
            "fn first(v: f32, min: f32, max: f32) -> f32 {\n    ((v - min) / (max - min)).clamp(0.0, 1.0)\n}\n"
            "fn second(v: f32, min: f32, max: f32) -> f32 {\n    ((v - min) / (max - min)).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-001", "UIK-VALUE-001"),
        ),
        (
            consumer,
            "the hand spelling of the projection is a second authority",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    ((value - min) / (max - min)).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "dividing a named span counts the same way",
            "fn thumb(value: f32) -> f32 {\n    let t = ((value - min) / span).clamp(0.0, 1.0);\n    t\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "the clamp written as min then max is the same fact",
            "fn fill(value: f64) -> f64 {\n    let t = ((value - baseline) / extent).min(1.0).max(0.0);\n    t\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "the projection split across statements is still a second authority",
            "fn badge(value: f32, floor: f32, ceiling: f32) -> f32 {\n    let span = ceiling - floor;\n    let raw = (value - floor) / span;\n    raw.clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            consumer,
            "a clamped ratio widened one statement later is caught too",
            "fn ramp(value: f32) -> f32 {\n    let t = (value - min) / range;\n    let u = t.clamp(0.0, 1.0);\n    u\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            consumer,
            "the owner call is the legal form",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    interval_fraction(value, min, max, 0.5)\n}\n",
            (),
        ),
        (
            consumer,
            "a guard plus the owner call keeps its own degenerate answer",
            "fn spark(value: f32) -> f32 {\n    let range = max - min;\n    if range <= 0.0 {\n        return 0.5;\n    }\n    interval_fraction(value, min, max, 0.0)\n}\n",
            (),
        ),
        (
            consumer,
            "an unclamped ratio is a different fact",
            "fn ratio(value: f32) -> f32 {\n    (value - min) / span\n}\n",
            (),
        ),
        (
            consumer,
            "a plain unit clamp without a range is untouched",
            "fn alpha(progress: f32) -> f32 {\n    progress.clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "a width divided by a point count is a slot budget, not a projection",
            "fn grid(width: f32) -> f32 {\n    let usable_w = (width - left_pad - right_pad).max(10.0);\n    let x_step = usable_w / (max_points - 1) as f32;\n    let t = (value - min) / span;\n    t.clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            consumer,
            "box geometry is not a numeric range",
            "fn fade(y: f32, top: f32) -> f32 {\n    let height = bottom - top;\n    (y / height).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "an inverted axis divides by a duration, not a value range",
            "fn step(elapsed: f32) -> f32 {\n    let delta = elapsed - start_time;\n    (delta / duration).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "a denominator renamed away from the vocabulary is still the projection",
            "fn gauge(value: f32) -> f32 {\n    let reach = ceiling - floor;\n    let shown = (value - floor) / reach;\n    shown.clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-003",),
        ),
        (
            consumer,
            "a renamed denominator clamped in the same statement is one report",
            "fn gauge(value: f32) -> f32 {\n    let reach = ceiling - floor;\n    ((value - floor) / reach).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-003",),
        ),
        (
            consumer,
            "a renamed denominator over a pixel row is a coverage ramp, not a value axis",
            "fn wash(row: f32) -> f32 {\n    let span = (baseline - top).max(1.0);\n    (1.0 - (row - top) / span).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "an inline divisor that is not a bound difference is not this law",
            "fn click_at(click: f32) -> f32 {\n    ((click - thumb / 2.0 - track_start) / (track_len - thumb)).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "a Newton correction divides by a derivative, not by a range",
            "fn solve(target: f64) -> f64 {\n    let mut parameter = target;\n    let inverse = 1.0 - parameter;\n    let slope = 3.0 * inverse * inverse + inverse - parameter;\n    let error = sample(parameter) - target;\n    parameter = (parameter - error / slope).clamp(0.0, 1.0);\n    parameter\n}\n",
            (),
        ),
        (
            consumer,
            "an unclamped ratio along a bound span is a different fact",
            "fn stroke(xa: i32, xb: i32) -> f32 {\n    let span = (xb - xa).abs().max(1) as f32;\n    let t = (x - xa).abs() as f32 / span;\n    a_y + (b_y - a_y) * t\n}\n",
            (),
        ),
        (
            consumer,
            "the same coverage ramp written with the projection's own words is still a ramp",
            "fn paint(row: f32) -> f32 {\n    let span = (bottom - top).max(1.0);\n    (1.0 - (row - top) / span).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            consumer,
            "a unit clamp with a type suffix is still a unit clamp",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    ((value - min) / (max - min)).clamp(0.0_f32, 1.0_f32)\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "a unit clamp written as 0. and 1. is the same breach",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    ((value - min) / span).clamp(0., 1.)\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "a saturating subtraction toward the floor divides the same span",
            "fn bar(value: f32, min: f32) -> f32 {\n    let t = value.saturating_sub(min) / span;\n    t.clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            consumer,
            "the unit ends handed back by branches is the same answer",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    let span = max - min;\n    if value <= min {\n        0.0\n    } else if value >= max {\n        1.0\n    } else {\n        (value - min) / span\n    }\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            consumer,
            "a reciprocal span multiplied back is the same division",
            "fn bar(value: f32, min: f32, max: f32) -> f32 {\n    let span = max - min;\n    let inverse = 1.0 / span;\n    ((value - min) * inverse).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-002",),
        ),
        (
            "zhuge-ui-core/src/value.rs",
            "the owner that defines the projection is exempt",
            "pub fn interval_fraction(value: f32, min: f32, max: f32) -> f32 {\n    ((value - min) / (max - min)).clamp(0.0, 1.0)\n}\n",
            (),
        ),
        (
            "music-frog/crates/widgets/src/value.rs",
            "a product copy of the definition is not exempt",
            "pub fn interval_fraction(value: f32, min: f32, max: f32) -> f32 {\n    ((value - min) / (max - min)).clamp(0.0, 1.0)\n}\n",
            ("UIK-VALUE-001",),
        ),
        (
            consumer,
            "prose quoting the arithmetic is not an implementation",
            "// It used to be ((value - min) / span).clamp(0.0, 1.0).\nfn draw() {}\n",
            (),
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
    print(f"interval normalization guard: self-test {total - failures}/{total} fixtures behaved as specified; {must_not_report} must not report")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="interval normalization uniqueness guard")
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
        print("interval normalization guard: no scan root; pass --root <tree>", file=sys.stderr)
        return 2
    violations = scan(roots)

    if args.write_baseline:
        written = write_baseline(args.write_baseline, violations)
        print(f"interval normalization guard: baseline written with {written} entries -> {args.write_baseline}")
        return 0

    baseline: dict[str, int] = {}
    if args.baseline:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"interval normalization guard: baseline unreadable: {error}", file=sys.stderr)
            return 2
        mismatch = provenance_mismatch(args.baseline)
        if mismatch:
            message = (
                "interval normalization guard: ledger was written against a different gate "
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
        print(f"interval normalization guard: stale baseline entry: {key}", file=sys.stderr)

    scanned = sum(len(rust_files(root)) for root in roots)
    if scanned == 0:
        print("interval normalization guard: no Rust source scanned — refusing to pass silently", file=sys.stderr)
        return 1
    covered = len(violations) - len(fresh)
    print(
        f"interval normalization guard: scanned={scanned} violations={len(violations)} "
        f"new={len(fresh)} baselined={covered} stale={len(stale)}"
    )
    return 1 if fresh and args.mode == "enforce" else 0


if __name__ == "__main__":
    raise SystemExit(main())
