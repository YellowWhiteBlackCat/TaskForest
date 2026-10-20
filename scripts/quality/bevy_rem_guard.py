#!/usr/bin/env python3
"""Enforce the Bevy frontend's rem sizing axis.

The Bevy frontend's type ladder — control/row height, icon extent, font size,
line height — is authored on the rem axis only: ``rem(palette.control_height_rem
…)``, ``rem(palette.icon_rem)`` and ``FontSize::Rem``. The palette's px
derivations (``control_height_px`` / ``icon_size_px``) exist for *pure math* —
row counting, virtual-window arithmetic, polyline segment geometry — and must
never be the source of a layout length, because a px length silently drops the
user's UI-size step.

This guard closes that hole mechanically:

* ``BEVY-REM-001`` — ``FontSize::Px`` is hard zero: a font size rides rem.
* ``BEVY-REM-002`` — a px derivation must not be the source of a layout
  length: not written directly into a ``Node`` length field, not carried into
  one by a ``let`` binding, and not handed to a function whose parameter feeds
  one.
* ``BEVY-REM-003`` — an icon scene adapter must not be handed a bare numeric
  extent: the palette's icon step is the only icon authority.

Layout length means the ``Node`` component's size fields (``width``,
``height``, the min/max pair, ``flex_basis`` and the box metrics). Chart
geometry — grid offsets, polyline segments, the inline text-flow box — is a
pixel contract and stays out of this rule; the inline flow box in particular
is the one engine-px seam and is fed by the palette's icon step.

It scans only ``crates/taskmanager-bevy-ui/src`` by default. Tests and Bevy's
own plugin internals are outside the production authoring contract. The
scanner masks comments and literals before checking balanced spans, so
documentation examples cannot satisfy or trip the rules accidentally.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from bevy_bsn_guard import mask_rust


# The palette's px derivations: pure math only, never a layout length source.
PX_DERIVATION = re.compile(r"\b(?:control_height_px|icon_size_px)\b")
# A font size that is not on the rem axis.
FONT_SIZE_PX = re.compile(r"\bFontSize\s*::\s*Px\b")
# The icon scene adapters; their extent comes from the palette, never a literal.
ICON_ADAPTER = re.compile(r"\b(?:icon_scene|inline_icon_scene)\s*\(")
# A bare numeric argument (no call, no path, no expression).
NUMERIC_ARGUMENT = re.compile(r"^[-+]?\d+(?:\.\d+)?$")
# `let NAME = <initializer>;`
LET_BINDING = re.compile(
    r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^;]+);", re.DOTALL
)
# `fn name(`
FUNCTION = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
# The `Node` layout component and the size fields the rem axis governs.
NODE_LITERAL = re.compile(r"\bNode\s*\{")
LENGTH_FIELDS = (
    "width",
    "height",
    "min_width",
    "min_height",
    "max_width",
    "max_height",
    "flex_basis",
    "row_gap",
    "column_gap",
    "gap",
    "padding",
    "margin",
    "border",
)


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str
    detail: str


@dataclass(frozen=True)
class Function:
    """One function definition and the parameter positions a length field uses."""

    name: str
    parameters: tuple[str, ...]
    length_parameters: frozenset[int]
    body_start: int
    body_end: int


def line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def matching_brace(masked: str, opening: int) -> int | None:
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return index + 1
    return None


def matching_paren(masked: str, opening: int) -> int | None:
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "(":
            depth += 1
        elif masked[index] == ")":
            depth -= 1
            if depth == 0:
                return index
    return None


def split_top_level(text: str) -> list[str]:
    """Split a comma separated list at top level only."""

    return [part for part, _offset in split_top_level_offsets(text, 0)]


def split_top_level_offsets(text: str, base: int) -> list[tuple[str, int]]:
    """Split a comma separated list at top level only, keeping offsets."""

    parts: list[tuple[str, int]] = []
    depth = 0
    start = 0
    for index, character in enumerate(text):
        if character in "([{":
            depth += 1
        elif character in ")]}":
            depth -= 1
        if character == "," and depth == 0:
            parts.append((text[start:index].strip(), base + start))
            start = index + 1
    tail = text[start:].strip()
    if tail:
        parts.append((tail, base + start))
    return parts


def node_literals(masked: str) -> list[tuple[int, int]]:
    """Balanced spans of the `Node` layout literals."""

    spans: list[tuple[int, int]] = []
    for match in NODE_LITERAL.finditer(masked):
        opening = masked.find("{", match.start(), match.end())
        end = matching_brace(masked, opening)
        if end is not None:
            spans.append((match.start(), end))
    return spans


def length_fields(masked: str, span: tuple[int, int]) -> list[tuple[str, str, int]]:
    """The size fields of one `Node` literal: name, value, value offset."""

    start, end = span
    opening = masked.find("{", start, end)
    if opening == -1:
        return []
    body = masked[opening + 1 : end - 1]
    fields: list[tuple[str, str, int]] = []
    for part, offset in split_top_level_offsets(body, opening + 1):
        name, separator, value = part.partition(":")
        field = name.strip()
        if not separator or field not in LENGTH_FIELDS:
            continue
        value_offset = offset + len(name) + 1
        fields.append((field, value.strip(), value_offset))
    return fields


def parameter_names(text: str) -> list[str]:
    """The bound identifier of every parameter, in declaration order."""

    names: list[str] = []
    for parameter in split_top_level(text):
        head = parameter.split(":", 1)[0].strip()
        head = head.removeprefix("mut ").strip()
        head = head.lstrip("&").lstrip("*").strip()
        if " " in head:
            head = head.split(" ", 1)[0].strip()
        if head and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", head):
            names.append(head)
    return names


def functions_of(masked: str) -> list[Function]:
    """Functions whose parameters feed a `Node` length field."""

    functions: list[Function] = []
    for match in FUNCTION.finditer(masked):
        opening = masked.find("(", match.end())
        if opening == -1:
            continue
        closing = matching_paren(masked, opening)
        if closing is None:
            continue
        body_opening = masked.find("{", closing)
        if body_opening == -1:
            continue
        body_end = matching_brace(masked, body_opening)
        if body_end is None:
            continue
        names = tuple(parameter_names(masked[opening + 1 : closing]))
        consumed: set[int] = set()
        for span in node_literals(masked[body_opening:body_end]):
            for _field, value, _offset in length_fields(
                masked[body_opening:body_end], span
            ):
                for index, name in enumerate(names):
                    if re.search(rf"\b{re.escape(name)}\b", value):
                        consumed.add(index)
        functions.append(
            Function(
                name=match.group(1),
                parameters=names,
                length_parameters=frozenset(consumed),
                body_start=body_opening,
                body_end=body_end,
            )
        )
    return functions


def px_derived_bindings(masked: str) -> dict[str, int]:
    """`let` bindings whose initializer carries a px derivation."""

    bindings: dict[str, int] = {}
    for match in LET_BINDING.finditer(masked):
        if PX_DERIVATION.search(match.group(2)):
            bindings[match.group(1)] = match.start()
    return bindings


def enclosing_function(functions: list[Function], offset: int) -> Function | None:
    """The innermost function whose body contains the offset."""

    candidates = [
        function
        for function in functions
        if function.body_start <= offset < function.body_end
    ]
    if not candidates:
        return None
    return min(candidates, key=lambda function: function.body_end - function.body_start)


def visible_bindings(
    bindings: dict[str, int], functions: list[Function], offset: int
) -> dict[str, int]:
    """The px-derived bindings visible at an offset.

    A binding is visible inside the function that declares it; the call flow
    covers the values that travel between functions.
    """

    scope = enclosing_function(functions, offset)
    if scope is None:
        return bindings
    return {
        name: start
        for name, start in bindings.items()
        if scope.body_start <= start < scope.body_end
    }


def px_source(expression: str, bindings: dict[str, int]) -> str | None:
    """The px-derived name or call a length expression carries, if any."""

    direct = PX_DERIVATION.search(expression)
    if direct:
        return direct.group(0)
    for name in sorted(bindings):
        if re.search(rf"\b{re.escape(name)}\b", expression):
            return name
    return None


def scan_file(path: Path, repository: Path) -> list[Violation]:
    original = path.read_text(encoding="utf-8")
    masked = mask_rust(original)
    bindings = px_derived_bindings(masked)
    functions = functions_of(masked)
    relative = path.relative_to(repository).as_posix()
    violations: list[Violation] = []

    for match in FONT_SIZE_PX.finditer(masked):
        violations.append(
            Violation(
                relative,
                line_number(original, match.start()),
                "BEVY-REM-001",
                "font size must ride the rem axis; FontSize::Px is hard zero",
            )
        )

    for span in node_literals(masked):
        for field, value, offset in length_fields(masked, span):
            source = px_source(value, visible_bindings(bindings, functions, offset))
            if source is None:
                continue
            violations.append(
                Violation(
                    relative,
                    line_number(original, offset),
                    "BEVY-REM-002",
                    f"length field '{field}' carries the px derivation '{source}'; "
                    "author the length on the rem axis",
                )
            )

    for match in ICON_ADAPTER.finditer(masked):
        opening = masked.find("(", match.start(), match.end())
        closing = matching_paren(masked, opening)
        if closing is None:
            continue
        for argument in split_top_level(masked[opening + 1 : closing]):
            if NUMERIC_ARGUMENT.match(argument):
                violations.append(
                    Violation(
                        relative,
                        line_number(original, match.start()),
                        "BEVY-REM-003",
                        "an icon extent must come from the palette icon step, "
                        "never from a bare number",
                    )
                )
                break
    return violations


def scan_call_flow(
    files: dict[Path, str], repository: Path
) -> list[Violation]:
    """A px derivation handed to a length-consuming parameter is a violation.

    The one-hop call flow closes the gap a same-scope binding cannot see: a row
    height computed by the caller and declared by the callee.
    """

    violations: list[Violation] = []
    definitions: dict[str, list[Function]] = {}
    for original in files.values():
        masked = mask_rust(original)
        for function in functions_of(masked):
            if function.length_parameters:
                definitions.setdefault(function.name, []).append(function)

    for path, original in files.items():
        masked = mask_rust(original)
        if not PX_DERIVATION.search(masked):
            continue
        bindings = px_derived_bindings(masked)
        functions = functions_of(masked)
        relative = path.relative_to(repository).as_posix()
        for name, candidates in definitions.items():
            if name not in masked:
                continue
            for match in re.finditer(rf"\b{re.escape(name)}\s*\(", masked):
                opening = masked.find("(", match.start(), match.end())
                closing = matching_paren(masked, opening)
                if closing is None:
                    continue
                arguments = split_top_level(masked[opening + 1 : closing])
                visible = visible_bindings(bindings, functions, match.start())
                for function in candidates:
                    if len(arguments) != len(function.parameters):
                        continue
                    for index in sorted(function.length_parameters):
                        if index >= len(arguments):
                            continue
                        source = px_source(arguments[index], visible)
                        if source is None:
                            continue
                        violations.append(
                            Violation(
                                relative,
                                line_number(original, match.start()),
                                "BEVY-REM-002",
                                f"'{name}' receives the px derivation '{source}' as "
                                "a layout length; author the length on the rem axis",
                            )
                        )
                        break
    return violations


def rust_files(root: Path) -> list[Path]:
    if root.is_file():
        return [root] if root.suffix == ".rs" else []
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def scan(repository: Path, root: Path) -> list[Violation]:
    violations: list[Violation] = []
    files: dict[Path, str] = {}
    for path in rust_files(root):
        original = path.read_text(encoding="utf-8")
        files[path] = original
        violations.extend(scan_file(path, repository))
    violations.extend(scan_call_flow(files, repository))
    return sorted(
        violations, key=lambda item: (item.path, item.line, item.code, item.detail)
    )


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="bevy-rem-guard-") as raw:
        repository = Path(raw)
        source = repository / "crates/taskmanager-bevy-ui/src"
        source.mkdir(parents=True)
        (source / "clean.rs").write_text(
            """
            fn scene(palette: &UiPalette) {
                bsn! { Node { height: rem(palette.control_height_rem) } }
                let height = palette.control_height_rem * 1.4;
                bsn! { Node { height: rem(height) } }
            }
            fn math(palette: &UiPalette) -> f32 {
                palette.control_height_px()
            }
            fn grid(top: f32) {
                bsn! { Node { top: px(top), height: px(1.0) } }
            }
            fn icon(palette: &UiPalette) {
                bsn! { Node { width: rem(palette.icon_rem) } }
            }
            // height: rem(palette.control_height_rem) icon_scene(id, 18.0, ink)
            """,
            encoding="utf-8",
        )
        (source / "violations.rs").write_text(
            """
            fn direct(palette: &UiPalette) {
                bsn! { Node { height: px(palette.control_height_px()) } }
            }
            fn bound(palette: &UiPalette) {
                let height = palette.control_height_px();
                bsn! { Node { height: px(height) } }
            }
            fn callee(row_height: f32) {
                bsn! { Node { height: px(row_height) } }
            }
            fn caller(palette: &UiPalette) {
                let row_height = palette.control_height_px();
                callee(row_height);
            }
            fn font() {
                let size = FontSize::Px(14.0);
            }
            fn icon(palette: &UiPalette) {
                bsn! { Node { width: px(18.0) } }
                let plate = icon_scene(IconId::Cpu, 18.0, palette.body_color);
            }
            """,
            encoding="utf-8",
        )
        found = scan(repository, source)
        codes = [item.code for item in found]
        expected = [
            "BEVY-REM-001",
            "BEVY-REM-002",
            "BEVY-REM-002",
            "BEVY-REM-002",
            "BEVY-REM-002",
            "BEVY-REM-003",
        ]
        if sorted(codes) != sorted(expected):
            print(f"self-test failed: expected {expected}, found {codes}", file=sys.stderr)
            return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--mode", choices=("report", "enforce"), default="enforce")
    parser.add_argument("--root", type=Path)
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    repository = Path(__file__).resolve().parents[2]
    root = args.root or Path("crates/taskmanager-bevy-ui/src")
    if not root.is_absolute():
        root = repository / root
    root = root.resolve()
    violations = scan(repository, root)
    for item in violations:
        print(f"{item.path}:{item.line}: {item.code}: {item.detail}")
    return int(bool(violations) and args.mode == "enforce")


if __name__ == "__main__":
    raise SystemExit(main())
