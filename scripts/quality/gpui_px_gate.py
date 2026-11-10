#!/usr/bin/env python3
"""GPUI / `taskmanager-ui` 生产树的 raw `px(<literal>)` 棘轮门。

UIFRAME-152 的机械执行器。律法正文在 TODO 台账（UIFRAME-152）：

* 生产树里每一条非零 raw `px(<literal>)` 都是**布局债务或布局契约**，必须在
  棘轮台账里有一条带类别与理由的登记；
* `px(0)` / `px(0.0)` 是复位，不是债务（与 Bevy 场景门同一律法）；
* 台账之外的新增（新组合或同组合计数增长）立即失败；
* 台账里已不存在的旧账（计数下降或组合消失）也失败——存量还清了就必须收台账，
  不允许台账替代码说谎；
* 注释与字符串字面量不参与（文档示例既不满足也不触发规则）。

台账每条登记 = 「文件 × 数值」组合 + 出现次数 + 类别 + 理由。类别把存量分成
**布局契约**（列宽/表面几何/图表几何/控件几何/行高/发丝线/几何运算/演示夹具）
与**待迁 token 债务**（`spacing-debt`：应迁 `tokens::SPACE_*` / `ICON_*` 的间距
与图标尺寸），后者按轮次迁移削减。

用法::

    python3 scripts/quality/gpui_px_gate.py --root <tree> [--root <tree2>] \\
        --baseline scripts/quality/gpui_px_ratchet.json          # 强制
    python3 scripts/quality/gpui_px_gate.py ... --write-baseline # 刷新台账
    python3 scripts/quality/gpui_px_gate.py ... --mode report    # 只报告
    python3 scripts/quality/gpui_px_gate.py --self-test          # 内建自检
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

BASELINE_VERSION = 1

REPOSITORY = Path(__file__).resolve().parents[2]

PX_LITERAL = re.compile(r"\bpx\s*\(\s*(?P<sign>-)?\s*(?P<num>\d+(?:\.\d+)?)\s*\)")

# `tokens::SPACE_*` 阶梯的成员值；阶梯外的 inset 也是债务，理由里注明需扩展阶梯。
SPACE_LADDER = {
    1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 13.0, 14.0,
    16.0, 18.0, 20.0, 24.0, 48.0, -8.0,
}

INSET_PROPERTY = re.compile(
    r"(?<!\w)(?:gap|p|pl|pr|pt|pb|mx|my|ml|mr|mt|mb|top|left|bottom|right|"
    r"snap_to_window_with_margin)\s*\(\s*(?:px\(|\[%|\[)"
)


@dataclass(frozen=True)
class Entry:
    """One ledgered 「文件 × 数值」 px-literal combo."""

    path: str
    value: str
    count: int
    category: str
    reason: str


def mask_noncode(text: str) -> str:
    """Mask comments and string/char literals without changing offsets.

    Doc examples and prose must neither satisfy nor trip the rule.
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
            literal = re.match(
                r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\r\n])'",
                text[index:],
            )
            if literal:
                _blank(chars, index, index + literal.end())
                index += literal.end()
            else:
                index += 1
            continue
        index += 1
    return "".join(chars)


def _blank(chars: list[str], start: int, end: int) -> None:
    for index in range(start, min(end, len(chars))):
        if chars[index] not in "\r\n":
            chars[index] = " "


def scan(roots: list[Path]) -> list[Entry]:
    """Collect every nonzero raw `px(<literal>)` combo under the roots."""

    combos: dict[tuple[str, str], dict[str, object]] = {}
    for root in roots:
        if not root.is_dir():
            raise SystemExit(f"root is not a directory: {root}")
        for path in sorted(root.rglob("*.rs")):
            resolved = path.resolve()
            try:
                key_path = resolved.relative_to(REPOSITORY).as_posix()
            except ValueError:
                key_path = resolved.as_posix()
            text = path.read_text(encoding="utf-8")
            masked = mask_noncode(text)
            lines = masked.splitlines()
            for number, line in enumerate(lines):
                for match in PX_LITERAL.finditer(line):
                    value = normalize_value(match.group("sign"), match.group("num"))
                    if value == "0.0":
                        continue
                    key = (key_path, value)
                    combo = combos.setdefault(
                        key, {"count": 0, "lines": [], "context": ""}
                    )
                    combo["count"] += 1
                    if len(combo["lines"]) < 4:
                        combo["lines"].append(" ".join(line.split()))
                    if not combo["context"]:
                        start = max(0, number - 3)
                        combo["context"] = " ⏎ ".join(
                            " ".join(lines[i].split()) for i in range(start, number + 1)
                        )[:400]
    entries = []
    for (path, value), combo in sorted(combos.items()):
        category, reason = classify(path, value, combo)
        entries.append(
            Entry(path=path, value=value, count=int(combo["count"]), category=category, reason=reason)
        )
    return entries


def normalize_value(sign: str | None, num: str) -> str:
    value = float(num)
    if not num.endswith(".0") and "." not in num:
        pass
    text = f"{value:.1f}"
    return f"-{text}" if sign else text


CONTEXT_BEFORE = re.compile(r"TableColumn::new|contract_spec\(")


def classify(path: str, value: str, combo: dict[str, object]) -> tuple[str, str]:
    """Assign one combo its category and per-entry reason.

    The rules read the call site's own vocabulary (the property that carries the
    `px` and the surrounding lines), so every ledger entry states what the
    literal is for and which side of the 布局契约 / token-debt axis it sits on.
    """

    lines: list[str] = list(combo["lines"])  # type: ignore[arg-type]
    context: str = str(combo["context"])  # type: ignore[arg-type]
    joined = " ⏎ ".join(lines)
    number = float(value)

    if CONTEXT_BEFORE.search(context) and re.search(r"\.width\(|width:|w\(", joined):
        return "column-width", f"表格列宽契约：{value}（TableColumn/contract_spec 默认或钳制）"
    if "row_height" in joined:
        return "row-height", f"行高契约：row_height = {value}"
    if re.search(r"\bconst\b", joined) or re.search(r"\bconst\b", context):
        if re.search(r"GRAPH|SECTION|STATS|COMPACT|CHART", context):
            return "chart-geometry", f"图表布局常量：{value}（perf_views 槽位几何）"
        return "geometry-math", f"布局常量：{value}（{path.split('/')[-1]} 的具名几何）"
    if "root/startup/" in path:
        return "demo-fixture", f"演示窗口几何：origin {value}（demo fixture，非生产布局）"
    if number in (-1.0,):
        return "hairline", f"发丝线重叠几何：px({value})（行边界对齐 hack）"
    if number == 1.0 and not re.search(r"if |clamp\(|changed|\.max\(|\.min\(", joined):
        return "hairline", f"发丝线：px(1.0) 分隔/描边"
    icon_context = bool(re.search(r"icon\(|IconId|icons_binding|icon_", joined + " " + context))
    if icon_context and re.search(r"\.size\(|\.w\(", joined):
        if number in (16.0, 18.0):
            return "spacing-debt", f"图标尺寸 px({value})：与 tokens::ICON_INLINE/STANDALONE 同值，待裁决迁移"
        return "icon-size", f"图标尺寸：size(px({value}))（组件图标几何）"
    if re.search(r"if |clamp\(|changed|\.max\(|\.min\(|offset|point\(|origin:", joined):
        return "geometry-math", f"几何运算：阈值/钳制/锚点 {value}"
    if re.search(r"bg\(fill|theme\.border", joined) and re.search(r"\.h\(|\.w\(", joined):
        return "geometry-math", f"分隔条几何：{value}（发丝线的承载高度/宽度）"
    if re.search(r"spread_radius|blur_radius", joined):
        return "geometry-math", f"阴影几何：{value}"
    if re.search(r"rounded\(|\.border\(", joined):
        return "geometry-math", f"圆角/描边几何：{value}"
    if INSET_PROPERTY.search(joined):
        if number in SPACE_LADDER:
            return "spacing-debt", f"组件内间距 px({value})：应迁 tokens::SPACE_*（待迁移削减）"
        return "spacing-debt", f"组件内间距 px({value})：阶梯外值，迁移时需扩展 tokens 阶梯或取整"
    if re.search(r"\.size\(", joined) and number in (10.0, 12.0, 13.0, 14.0, 16.0, 18.0, 22.0):
        return "icon-size", f"图标/指示点尺寸：size(px({value}))（组件几何）"
    if re.search(r"\.h\(|height:|\.w\(", joined) and number in (20.0, 24.0, 26.0, 28.0, 30.0, 34.0, 36.0, 42.0):
        return "control-geometry", f"控件几何：高 {value}（按钮/行/输入高度契约）"
    if "dialog" in path:
        return "surface-geometry", f"表面几何：{value}（模态对话框宽度契约）"
    if "perf_views/" in path:
        if re.search(r"\.max_w\(", joined):
            return "surface-geometry", f"表面几何：{value}（perf 面板/卡片宽度上限）"
        return "chart-geometry", f"图表/条形几何：{value}（perf_views 布局）"
    if re.search(r"\.w\(|\.max_w\(|\.min_w\(|width: Some|max_height:|AdaptiveGrid", joined):
        return "surface-geometry", f"表面几何：{value}（模态/面板/网格宽度或上限）"
    return "surface-geometry", f"布局几何：{value}（面契约，见调用点）"


def load_ledger(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"baseline ledger not found: {path}")
    except json.JSONDecodeError as error:
        raise SystemExit(f"baseline ledger is not valid JSON: {path}: {error}")
    if payload.get("version") != BASELINE_VERSION:
        raise SystemExit(f"baseline ledger version mismatch in {path}")
    return payload


def gate_sha256() -> str:
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def enforce(entries: list[Entry], ledger: dict, ledger_path: Path) -> int:
    recorded = ledger.get("gate", {}).get("sha256")
    if not recorded:
        print(f"FAIL gpui-px-gate: ledger {ledger_path} does not record the gate digest")
        return 1
    if recorded != gate_sha256():
        print(
            f"FAIL gpui-px-gate: ledger pins gate digest {str(recorded)[:12]}… but this gate is "
            f"{gate_sha256()[:12]}…; refresh the ledger with --write-baseline"
        )
        return 1

    ledgered = {
        (entry["path"], entry["value"]): entry
        for entry in ledger.get("entries", [])
        if isinstance(entry, dict) and "path" in entry and "value" in entry
    }
    current = {(entry.path, entry.value): entry for entry in entries}

    violations: list[str] = []
    for key, entry in current.items():
        if key not in ledgered:
            violations.append(
                f"NEW px literal {entry.value} in {entry.path} (×{entry.count}) — "
                f"migrate to a token or add a justified ledger entry"
            )
            continue
        recorded_entry = ledgered[key]
        if entry.count > int(recorded_entry.get("count", 0)):
            violations.append(
                f"GROWN px literal {entry.value} in {entry.path}: "
                f"ledger {recorded_entry.get('count')} → now {entry.count}"
            )
    for key, recorded_entry in ledgered.items():
        if key not in current:
            violations.append(
                f"STALE ledger entry px({recorded_entry.get('value')}) in {key[0]} — "
                f"the debt is gone, shrink the ledger (--write-baseline)"
            )
        elif current[key].count < int(recorded_entry.get("count", 0)):
            violations.append(
                f"SHRUNK px literal {current[key].value} in {key[0]}: "
                f"ledger {recorded_entry.get('count')} → now {current[key].count} — refresh the ledger"
            )

    if violations:
        print(f"FAIL gpui-px-gate: {len(violations)} ratchet violation(s)")
        for violation in violations[:40]:
            print(f"  {violation}")
        if len(violations) > 40:
            print(f"  … and {len(violations) - 40} more")
        return 1
    print(
        f"gpui-px-gate: pass — {len(current)} ledgered combos, "
        f"{sum(entry.count for entry in current.values())} literal sites, "
        f"{sum(1 for entry in current.values() if entry.category == 'spacing-debt')} spacing-debt"
    )
    return 0


def write_baseline(entries: list[Entry], path: Path) -> None:
    payload = {
        "version": BASELINE_VERSION,
        "gate": {"sha256": gate_sha256()},
        "entries": [
            {
                "path": entry.path,
                "value": entry.value,
                "count": entry.count,
                "category": entry.category,
                "reason": entry.reason,
            }
            for entry in entries
        ],
    }
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(
        f"wrote {path}: {len(entries)} combos, "
        f"{sum(1 for entry in entries if entry.category == 'spacing-debt')} spacing-debt"
    )


def self_test() -> int:
    """Bite on new/grown/stale, exempt zeros and doc comments, count negatives."""

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "a.rs").write_text(
            "fn f() {\n"
            "    div().w(px(28.0));\n"
            "    div().h(px(0.0));\n"
            "    // px(99.0) in a comment\n"
            "    /// doc px(88.0)\n"
            "    let s = \"px(77.0)\";\n"
            "    div().w(px(-1.0));\n"
            "}\n",
            encoding="utf-8",
        )
        entries = scan([root])
        by_value = {entry.value: entry for entry in entries}
        if set(by_value) != {"28.0", "-1.0"}:
            print(f"self-test: expected exactly 28.0/-1.0, got {sorted(by_value)}")
            return 1
        if by_value["-1.0"].category != "hairline":
            print(f"self-test: px(-1.0) should be hairline, got {by_value['-1.0'].category}")
            return 1

        baseline = root / "baseline.json"
        write_baseline(entries, baseline)
        if enforce(scan([root]), json.loads(baseline.read_text(encoding="utf-8")), baseline) != 0:
            print("self-test: clean stock must pass its own baseline")
            return 1

        (root / "a.rs").write_text(
            "fn f() {\n"
            "    div().w(px(28.0));\n"
            "    div().w(px(28.0));\n"
            "    div().w(px(50.0));\n"
            "}\n",
            encoding="utf-8",
        )
        grown = enforce(scan([root]), json.loads(baseline.read_text(encoding="utf-8")), baseline)
        if grown != 1:
            print("self-test: grown count + new combo must fail")
            return 1

        (root / "a.rs").write_text("fn f() {\n    div().w(px(28.0));\n}\n", encoding="utf-8")
        shrunk = enforce(scan([root]), json.loads(baseline.read_text(encoding="utf-8")), baseline)
        if shrunk != 1:
            print("self-test: shrunk/stale ledger must fail (shrink the ledger)")
            return 1
    print("self-test: pass")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", action="append", type=Path, default=[])
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--write-baseline", action="store_true")
    parser.add_argument("--mode", choices=("enforce", "report"), default="enforce")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()

    if arguments.self_test:
        return self_test()
    if not arguments.root:
        parser.error("--root is required (repeatable)")

    entries = scan(arguments.root)
    if arguments.write_baseline:
        if not arguments.baseline:
            parser.error("--write-baseline requires --baseline")
        write_baseline(entries, arguments.baseline)
        return 0
    if not arguments.baseline:
        parser.error("--baseline is required for enforce/report mode")
    ledger = load_ledger(arguments.baseline)
    result = enforce(entries, ledger, arguments.baseline)
    if arguments.mode == "report":
        return 0
    return result


if __name__ == "__main__":
    sys.exit(main())
