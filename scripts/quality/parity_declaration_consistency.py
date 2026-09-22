#!/usr/bin/env python3
"""Declaration/implementation consistency guard for the four-frontend matrix.

The CORE-04 matrix already proves *declaration discipline*: every frontend must
declare a [`SurfaceDecision`] for every ``ProductIntent`` (see
``crates/taskmanager-ui-contract/src/functional.rs`` and
``functional_findings``). It does not prove that the declaration is *true*.

This guard closes that gap by comparing each frontend's declaration against the
crate's own source tree:

``unsupported_but_implemented`` (``PARITY-DECL-001``)
    An intent declared ``Unsupported`` must not have real production evidence in
    ``<crate>/src``. If the intent name (or a known action alias such as
    ``ExportDiagnosticReport`` for ``DiagnosticBundle``) appears outside the
    declaration file, the "unsupported" claim is contradicted.

``route_missing`` (``PARITY-DECL-002``)
    A ``Reference`` / ``Shared`` / ``Local`` / ``AcceptedDifference`` decision
    names a route that must be findable in the crate. The route string itself is
    preferred evidence (it is usually pinned by the crate's own declaration
    tests); when the route is only a semantic name, distinctive route segments
    and adjacent-segment identifiers (``services.details-column`` ->
    ``details_column``) are accepted. This fallback is deliberately lenient: the
    guard is a *contradiction detector*, not a proof of behaviour. Known
    false-positive risk: a generic segment such as ``details`` or ``gpu`` can
    match unrelated code; use the baseline for anything the owner accepts.

The guard never edits source. It prints findings and exits non-zero when any
finding is not covered by ``--baseline``.

Baseline
--------
Known-but-unfixed contradictions are recorded in a JSON file so parallel
workstreams are not blocked. Each entry names the finding and a ``remove_when``
condition::

    {
      "entries": [
        {
          "frontend": "tui",
          "intent": "DiagnosticBundle",
          "kind": "unsupported_but_implemented",
          "reason": "TUI diagnostic export exists while the declaration still says Unsupported",
          "remove_when": "functional.rs declares a supported route and the Unsupported test is removed"
        }
      ]
    }

A baseline entry that no longer matches any finding is reported as stale (a
warning by default; a failure with ``--strict-baseline``).

Usage::

    python3 scripts/quality/parity_declaration_consistency.py
    python3 scripts/quality/parity_declaration_consistency.py --json
    python3 scripts/quality/parity_declaration_consistency.py --baseline .private/parity-baseline.json
    python3 scripts/quality/parity_declaration_consistency.py --print-baseline-template
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

# ---------------------------------------------------------------------------
# Finding codes
# ---------------------------------------------------------------------------

UNSUPPORTED_BUT_IMPLEMENTED = "unsupported_but_implemented"
ROUTE_MISSING = "route_missing"
DECLARATION_UNPARSED = "declaration_unparsed"

FINDING_CODES = {
    UNSUPPORTED_BUT_IMPLEMENTED: "PARITY-DECL-001",
    ROUTE_MISSING: "PARITY-DECL-002",
    DECLARATION_UNPARSED: "PARITY-DECL-000",
}

# Frontends that own a CORE-04 declaration. Bevy's product crate is
# ``taskmanager-bevy-ui``; the contract crate itself is not a frontend.
FRONTENDS: tuple[tuple[str, str, str], ...] = (
    (
        "gpui",
        "crates/taskmanager-gpui",
        "crates/taskmanager-gpui/src/gpui_app/functional.rs",
    ),
    (
        "iced",
        "crates/taskmanager-iced",
        "crates/taskmanager-iced/src/functional.rs",
    ),
    (
        "bevy",
        "crates/taskmanager-bevy-ui",
        "crates/taskmanager-bevy-ui/src/functional.rs",
    ),
    (
        "tui",
        "crates/taskmanager-tui",
        "crates/taskmanager-tui/src/functional.rs",
    ),
)

SUPPORTED_KINDS = frozenset(
    {"Reference", "Shared", "Local", "AcceptedDifference", "RouteLiteral"}
)

# ---------------------------------------------------------------------------
# Declaration parsing
# ---------------------------------------------------------------------------

DIRECT_ARM = re.compile(
    r"ProductIntent::(\w+)\s*=>\s*SurfaceDecision::(\w+)\s*\{([^{}]*)\}"
)
STRING_ARM = re.compile(r'ProductIntent::(\w+)\s*=>\s*"([^"]*)"')
ROUTE_FIELD = re.compile(r'route\s*:\s*"([^"]*)"')
REASON_FIELD = re.compile(r'reason\s*:\s*"([^"]*)"')


@dataclass(frozen=True)
class Decision:
    """One frontend's parsed declaration for a single ``ProductIntent``."""

    intent: str
    kind: str
    route: str | None
    reason: str | None


def mask_rust_comments(text: str) -> str:
    """Blank out Rust line/block comments while preserving line breaks.

    Evidence must be code, not prose: a comment that mentions an intent must not
    count as an implementation. Nested block comments are handled.
    """

    chars = list(text)
    index = 0
    block_depth = 0
    while index < len(chars):
        pair = "".join(chars[index : index + 2])
        if block_depth:
            if pair == "/*":
                chars[index] = chars[index + 1] = " "
                block_depth += 1
                index += 2
            elif pair == "*/":
                chars[index] = chars[index + 1] = " "
                block_depth -= 1
                index += 2
            else:
                if chars[index] not in "\r\n":
                    chars[index] = " "
                index += 1
            continue
        if pair == "//":
            chars[index] = chars[index + 1] = " "
            index += 2
            while index < len(chars) and chars[index] not in "\r\n":
                chars[index] = " "
                index += 1
            continue
        if pair == "/*":
            chars[index] = chars[index + 1] = " "
            block_depth = 1
            index += 2
            continue
        index += 1
    return "".join(chars)


def parse_declaration(text: str) -> list[Decision]:
    """Parse every ``ProductIntent`` arm from one declaration file.

    Handles both shapes used in the repository:

    * ``ProductIntent::X => SurfaceDecision::Kind { route: "...", reason: "..." }``
      (Iced, Bevy, TUI), and
    * ``ProductIntent::X => "dotted.route"`` inside a reference-route helper
      (GPUI), where the enclosing entry constructs ``SurfaceDecision::Reference``.
    """

    masked = mask_rust_comments(text)
    decisions: dict[str, Decision] = {}
    for match in DIRECT_ARM.finditer(masked):
        intent, kind, body = match.group(1), match.group(2), match.group(3)
        route_match = ROUTE_FIELD.search(body)
        reason_match = REASON_FIELD.search(body)
        decisions[intent] = Decision(
            intent=intent,
            kind=kind,
            route=route_match.group(1) if route_match else None,
            reason=reason_match.group(1) if reason_match else None,
        )
    reference_helper = "SurfaceDecision::Reference" in masked
    for match in STRING_ARM.finditer(masked):
        intent, route = match.group(1), match.group(2)
        if intent in decisions:
            continue
        decisions[intent] = Decision(
            intent=intent,
            kind="Reference" if reference_helper else "RouteLiteral",
            route=route,
            reason=None,
        )
    return list(decisions.values())


# ---------------------------------------------------------------------------
# Route / intent evidence lexicons
# ---------------------------------------------------------------------------

# Domain and structural words that are shared by many routes and therefore carry
# little signal. Kept out of the weak (segment) evidence tier.
STOPWORDS = frozenset(
    {
        "dashboard",
        "services",
        "service",
        "system",
        "root",
        "about",
        "header",
        "footer",
        "first",
        "run",
        "page",
        "panel",
        "modal",
        "overlay",
        "dialog",
        "shell",
        "performance",
        "health",
        "processes",
        "process",
        "current",
        "window",
        "common",
        "local",
        "shared",
        "test",
        "self",
        "view",
    }
)

# Known action / command names that implement an intent without repeating the
# intent's own type name. These are the "same-named action, route string, command
# palette entry" anchors from the governance plan. Extend deliberately: every
# entry is a claim about real evidence, not a suppression.
INTENT_ALIASES: dict[str, tuple[str, ...]] = {
    "DiagnosticBundle": (
        "DiagnosticReport",
        "diagnostic_report",
        "ExportDiagnosticReport",
        "export_diagnostic_report",
        "diagnostics.bundle",
    ),
    "CurrentWindowScreenshot": (
        "CurrentWindowScreenshot",
        "current_window_screenshot",
        "screenshot",
        "capture_current_window",
    ),
    "FirstRunSetup": (
        "FirstRunSetup",
        "first_run_setup",
        "first_run",
        "first-run",
    ),
}


def to_snake_case(name: str) -> str:
    first = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", first).lower()


def to_pascal_case(segments: list[str]) -> str:
    return "".join(segment[:1].upper() + segment[1:] for segment in segments)


def intent_candidates(intent: str) -> list[str]:
    """Strong evidence needles for an intent's implementation."""

    snake = to_snake_case(intent)
    candidates = [
        intent,
        snake,
        snake.upper(),
        snake.replace("_", "-"),
    ]
    candidates.extend(INTENT_ALIASES.get(intent, ()))
    # De-duplicate while preserving order.
    seen: set[str] = set()
    ordered: list[str] = []
    for candidate in candidates:
        if candidate and candidate not in seen:
            seen.add(candidate)
            ordered.append(candidate)
    return ordered


def route_candidates(route: str) -> tuple[list[str], list[str]]:
    """Return ``(strong, weak)`` evidence needles for a declared route.

    ``strong`` needles are the literal route and its underscore form: exact
    corroboration. ``weak`` needles are distinctive segments and adjacent-segment
    identifiers accepted as a lenient fallback for semantic-only routes.
    """

    segments = [segment for segment in re.split(r"[.\-]", route) if segment]
    strong = [route, route.replace(".", "_").replace("-", "_")]
    if segments:
        strong.append(to_pascal_case(segments))

    weak: list[str] = []
    for segment in segments:
        if len(segment) >= 4 and segment not in STOPWORDS:
            weak.append(segment)
    # Adjacent-segment identifiers stay evidence even when one part is a generic
    # domain word: ``first-run`` -> ``first_run`` is distinctive in a way that
    # the bare ``run`` never is.
    for left, right in zip(segments, segments[1:]):
        weak.append(f"{left}_{right}")
    for first, second, third in zip(segments, segments[1:], segments[2:]):
        weak.append(f"{first}_{second}_{third}")

    def dedupe(items: list[str]) -> list[str]:
        seen: set[str] = set()
        ordered: list[str] = []
        for item in items:
            if item not in seen:
                seen.add(item)
                ordered.append(item)
        return ordered

    return dedupe(strong), dedupe(weak)


# ---------------------------------------------------------------------------
# Evidence index
# ---------------------------------------------------------------------------


@dataclass
class Evidence:
    """Masked source text for a crate, split by scope."""

    root: Path
    # Production evidence: <crate>/src only, declaration file excluded.
    src: list[tuple[Path, str]]
    # Corroboration evidence: the whole crate, declaration file excluded.
    all: list[tuple[Path, str]]

    def locate(
        self, needles: list[str], scope: str = "all"
    ) -> tuple[Path, int, str] | None:
        files = self.src if scope == "src" else self.all
        for needle in needles:
            for path, text in files:
                if needle not in text:
                    continue
                line = text.count("\n", 0, text.index(needle)) + 1
                return (path, line, needle)
        return None


def iter_rust_files(root: Path, exclude: Path) -> list[Path]:
    if not root.is_dir():
        return []
    files: list[Path] = []
    for path in root.rglob("*.rs"):
        if not path.is_file() or path == exclude:
            continue
        if "target" in path.parts:
            continue
        files.append(path)
    return sorted(files)


def build_evidence(crate_dir: Path, declaration: Path) -> Evidence:
    src_files = iter_rust_files(crate_dir / "src", declaration)
    all_files = iter_rust_files(crate_dir, declaration)
    src = [(path, mask_rust_comments(path.read_text(encoding="utf-8"))) for path in src_files]
    # Reuse the src read for files that also appear in the whole-crate scope.
    cached = dict(src)
    all_text = [
        (path, cached[path] if path in cached else mask_rust_comments(path.read_text(encoding="utf-8")))
        for path in all_files
    ]
    return Evidence(root=crate_dir, src=src, all=all_text)


# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Finding:
    frontend: str
    intent: str
    kind: str
    decision: str
    detail: str
    evidence: str | None = None

    @property
    def code(self) -> str:
        return FINDING_CODES.get(self.kind, "PARITY-DECL-099")

    def key(self) -> tuple[str, str, str]:
        return (self.frontend, self.intent, self.kind)

    def render(self) -> str:
        location = f" [{self.evidence}]" if self.evidence else ""
        return f"{self.code} {self.frontend} {self.intent} {self.kind}: {self.detail}{location}"

    def as_dict(self) -> dict[str, str | None]:
        return {
            "code": self.code,
            "frontend": self.frontend,
            "intent": self.intent,
            "kind": self.kind,
            "decision": self.decision,
            "detail": self.detail,
            "evidence": self.evidence,
        }


def analyze_frontend(
    name: str, crate_dir: Path, declaration: Path, repository: Path
) -> list[Finding]:
    findings: list[Finding] = []
    if not declaration.is_file():
        findings.append(
            Finding(
                frontend=name,
                intent="*",
                kind=DECLARATION_UNPARSED,
                decision="missing",
                detail=f"declaration file not found: {_relative(declaration, repository)}",
            )
        )
        return findings

    decisions = parse_declaration(declaration.read_text(encoding="utf-8"))
    if not decisions:
        findings.append(
            Finding(
                frontend=name,
                intent="*",
                kind=DECLARATION_UNPARSED,
                decision="unparsed",
                detail=f"no ProductIntent arms parsed from {_relative(declaration, repository)}",
            )
        )
        return findings

    evidence = build_evidence(crate_dir, declaration)
    for decision in sorted(decisions, key=lambda item: item.intent):
        if decision.kind == "Unsupported":
            needles = intent_candidates(decision.intent)
            hit = evidence.locate(needles, scope="src")
            if hit is not None:
                path, line, needle = hit
                findings.append(
                    Finding(
                        frontend=name,
                        intent=decision.intent,
                        kind=UNSUPPORTED_BUT_IMPLEMENTED,
                        decision="Unsupported",
                        detail=(
                            "declared Unsupported but production source implements it "
                            f"(matched '{needle}')"
                        ),
                        evidence=f"{_relative(path, repository)}:{line}",
                    )
                )
            continue

        if decision.kind in SUPPORTED_KINDS:
            if not decision.route:
                findings.append(
                    Finding(
                        frontend=name,
                        intent=decision.intent,
                        kind=ROUTE_MISSING,
                        decision=decision.kind,
                        detail=f"{decision.kind} decision has no route",
                    )
                )
                continue
            strong, weak = route_candidates(decision.route)
            # Strong evidence may live anywhere in the crate (declaration tests
            # pin route strings); weak segment evidence is accepted as well.
            hit = evidence.locate(strong, scope="all")
            if hit is None:
                hit = evidence.locate(weak, scope="all")
            if hit is None:
                findings.append(
                    Finding(
                        frontend=name,
                        intent=decision.intent,
                        kind=ROUTE_MISSING,
                        decision=decision.kind,
                        detail=(
                            f"{decision.kind} route '{decision.route}' has no evidence "
                            "in the crate"
                        ),
                    )
                )
    return findings


def _relative(path: Path, repository: Path) -> str:
    try:
        return path.relative_to(repository).as_posix()
    except ValueError:
        return path.as_posix()


# ---------------------------------------------------------------------------
# Baseline
# ---------------------------------------------------------------------------


@dataclass
class Baseline:
    entries: list[dict[str, str]]

    def match(self, finding: Finding) -> dict[str, str] | None:
        for entry in self.entries:
            if entry.get("frontend") != finding.frontend:
                continue
            if entry.get("intent") != finding.intent:
                continue
            if entry.get("kind") != finding.kind:
                continue
            return entry
        return None


def load_baseline(path: Path) -> Baseline:
    data = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(data, dict):
        raw_entries = data.get("entries", [])
    elif isinstance(data, list):
        raw_entries = data
    else:
        raise ValueError("baseline must be a JSON object with an 'entries' array")
    entries = []
    for raw in raw_entries:
        if not isinstance(raw, dict):
            raise ValueError("each baseline entry must be a JSON object")
        for key in ("frontend", "intent", "kind"):
            if key not in raw:
                raise ValueError(f"baseline entry missing required key '{key}': {raw!r}")
        entries.append(
            {
                "frontend": str(raw["frontend"]),
                "intent": str(raw["intent"]),
                "kind": str(raw["kind"]),
                "reason": str(raw.get("reason", "")),
                "remove_when": str(raw.get("remove_when", "")),
            }
        )
    return Baseline(entries=entries)


def baseline_template(findings: list[Finding]) -> str:
    entries = [
        {
            "frontend": finding.frontend,
            "intent": finding.intent,
            "kind": finding.kind,
            "reason": "",
            "remove_when": "",
        }
        for finding in findings
    ]
    return json.dumps({"entries": entries}, indent=2) + "\n"


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def collect_findings(repository: Path) -> list[Finding]:
    findings: list[Finding] = []
    for name, crate, declaration in FRONTENDS:
        findings.extend(
            analyze_frontend(
                name,
                (repository / crate).resolve(),
                (repository / declaration).resolve(),
                repository,
            )
        )
    return findings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    default_root = Path(__file__).resolve().parents[2]
    parser.add_argument("--root", type=Path, default=default_root, help="repository root")
    parser.add_argument("--json", action="store_true", help="emit JSON findings")
    parser.add_argument("--baseline", type=Path, help="JSON file of accepted contradictions")
    parser.add_argument(
        "--strict-baseline",
        action="store_true",
        help="fail when a baseline entry no longer matches any finding",
    )
    parser.add_argument(
        "--print-baseline-template",
        action="store_true",
        help="print a baseline skeleton for the current findings and exit 0",
    )
    args = parser.parse_args(argv)

    repository = args.root.resolve()
    findings = collect_findings(repository)

    if args.print_baseline_template:
        sys.stdout.write(baseline_template(findings))
        return 0

    baseline = Baseline(entries=[])
    if args.baseline is not None:
        try:
            baseline = load_baseline(args.baseline)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            print(f"parity-declaration-consistency: invalid baseline: {error}", file=sys.stderr)
            return 2

    suppressed: list[Finding] = []
    active: list[Finding] = []
    for finding in findings:
        if baseline.match(finding) is not None:
            suppressed.append(finding)
        else:
            active.append(finding)

    matched_keys = {finding.key() for finding in findings}
    stale = [
        entry
        for entry in baseline.entries
        if (entry["frontend"], entry["intent"], entry["kind"]) not in matched_keys
    ]

    if args.json:
        payload = {
            "findings": [finding.as_dict() for finding in active],
            "suppressed": [finding.as_dict() for finding in suppressed],
            "stale_baseline": stale,
        }
        print(json.dumps(payload, indent=2))
    else:
        for finding in active:
            print(finding.render())
        for finding in suppressed:
            print(f"suppressed-by-baseline: {finding.render()}")
        for entry in stale:
            print(
                "stale-baseline: "
                f"{entry['frontend']} {entry['intent']} {entry['kind']} "
                f"(remove_when: {entry['remove_when'] or 'unspecified'})",
                file=sys.stderr,
            )
        print(
            "parity-declaration-consistency: "
            f"{'FAIL' if active else 'PASS'} "
            f"({len(active)} finding(s), {len(suppressed)} suppressed, {len(stale)} stale)"
        )

    if active:
        return 1
    if stale and args.strict_baseline:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
