#!/usr/bin/env python3
"""Self-test for ``parity_declaration_consistency.py``.

Pure Python assertions, no pytest dependency. The test builds synthetic crate
trees in a temporary directory and proves that the guard:

* accepts a compliant declaration/implementation pair,
* reports ``unsupported_but_implemented`` when production source contradicts an
  ``Unsupported`` decision (the TUI ``DiagnosticBundle`` class of drift),
* reports ``route_missing`` when a ``Local`` route has no evidence,
* honours the corroboration scope (a route pinned only in the crate's own
  declaration test still counts),
* ignores intent names that appear only in comments,
* suppresses baselined findings and reports stale baseline entries,
* returns the documented exit codes end to end.

Run::

    python3 scripts/quality/test_parity_declaration_consistency.py
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
from pathlib import Path

GUARD_PATH = Path(__file__).with_name("parity_declaration_consistency.py")


def load_guard():
    """Import the guard module by path (it is a script, not a package)."""

    spec = importlib.util.spec_from_file_location(
        "parity_declaration_consistency", GUARD_PATH
    )
    module = importlib.util.module_from_spec(spec)
    # dataclasses resolves the module through sys.modules during class creation.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


@contextlib.contextmanager
def quiet():
    """Silence both streams for calls whose result is asserted, not printed."""

    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
        io.StringIO()
    ):
        yield


DIRECT_DECLARATION = """
use taskmanager_ui_contract::{ProductIntent, SurfaceDecision};

const fn decision(intent: ProductIntent) -> SurfaceDecision {
    match intent {
        ProductIntent::DiagnosticBundle => SurfaceDecision::AcceptedDifference {
            route: "diagnostics.report",
            reason: "synthetic accepted difference",
        },
        ProductIntent::ServiceLogExport => SurfaceDecision::Local {
            route: "services.log-panel.export",
        },
        ProductIntent::CurrentWindowScreenshot => SurfaceDecision::Unsupported {
            reason: "synthetic unsupported",
        },
    }
}
"""


def test_parser_handles_both_shapes(guard) -> None:
    direct = """
    fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::Alpha => SurfaceDecision::Local { route: "alpha.route" },
            ProductIntent::Beta => SurfaceDecision::Unsupported { reason: "nope" },
        }
    }
    """
    parsed = {item.intent: item for item in guard.parse_declaration(direct)}
    assert parsed["Alpha"].kind == "Local", parsed["Alpha"]
    assert parsed["Alpha"].route == "alpha.route"
    assert parsed["Beta"].kind == "Unsupported"
    assert parsed["Beta"].reason == "nope"

    helper = """
    fn declaration() -> Option<&'static str> {
        let _ = SurfaceDecision::Reference { route: reference_route(intent) };
        reference_route(intent)
    }

    const fn reference_route(intent: ProductIntent) -> &'static str {
        match intent {
            ProductIntent::Gamma => "gamma.route",
            ProductIntent::Delta => "delta.route",
        }
    }
    """
    parsed_helper = {item.intent: item for item in guard.parse_declaration(helper)}
    assert parsed_helper["Gamma"].kind == "Reference", parsed_helper["Gamma"]
    assert parsed_helper["Gamma"].route == "gamma.route"
    assert parsed_helper["Delta"].kind == "Reference"


def test_compliant_sample_passes(guard) -> None:
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, DIRECT_DECLARATION)
        write(
            crate / "src" / "diagnostic_report.rs",
            'pub const ROUTE: &str = "diagnostics.report";\n',
        )
        write(
            crate / "tests" / "functional_tests.rs",
            'fn pins_route() { assert_eq!(route, "services.log-panel.export"); }\n',
        )
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert findings == [], findings


def test_unsupported_but_implemented_is_reported(guard) -> None:
    declaration_text = """
    const fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {
                reason: "the shape has no diagnostic surface",
            },
        }
    }
    """
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, declaration_text)
        write(
            crate / "src" / "diagnostic_report.rs",
            "use taskmanager_core::DiagnosticBundlePlan;\n"
            "pub fn export_diagnostic_report() { let _ = DiagnosticBundlePlan::prepare; }\n",
        )
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert len(findings) == 1, findings
        finding = findings[0]
        assert finding.kind == guard.UNSUPPORTED_BUT_IMPLEMENTED, finding
        assert finding.code == "PARITY-DECL-001", finding
        assert "diagnostic_report.rs" in (finding.evidence or ""), finding


def test_unsupported_alias_evidence_is_reported(guard) -> None:
    declaration_text = """
    const fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {
                reason: "no bundle surface",
            },
        }
    }
    """
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, declaration_text)
        # Only the alias appears; the intent type name does not.
        write(
            crate / "src" / "palette.rs",
            "pub enum Action { ExportDiagnosticReport }\n",
        )
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert len(findings) == 1, findings
        assert findings[0].kind == guard.UNSUPPORTED_BUT_IMPLEMENTED, findings[0]


def test_route_missing_is_reported(guard) -> None:
    declaration_text = """
    const fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::ServiceDetails => SurfaceDecision::Local {
                route: "services.details-column",
            },
        }
    }
    """
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, declaration_text)
        write(crate / "src" / "lib.rs", "pub fn noop() {}\n")
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert len(findings) == 1, findings
        finding = findings[0]
        assert finding.kind == guard.ROUTE_MISSING, finding
        assert finding.code == "PARITY-DECL-002", finding
        assert "services.details-column" in finding.detail, finding


def test_route_pinned_in_crate_test_counts(guard) -> None:
    declaration_text = """
    const fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::ServiceDetails => SurfaceDecision::Local {
                route: "services.details-column",
            },
        }
    }
    """
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, declaration_text)
        write(crate / "src" / "lib.rs", "pub fn noop() {}\n")
        write(
            crate / "tests" / "functional_tests.rs",
            'fn pins_route() { assert_eq!(route, "services.details-column"); }\n',
        )
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert findings == [], findings


def test_comments_are_not_evidence(guard) -> None:
    declaration_text = """
    const fn decision(intent: ProductIntent) -> SurfaceDecision {
        match intent {
            ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {
                reason: "no bundle surface",
            },
        }
    }
    """
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        crate = root / "crates" / "taskmanager-demo"
        declaration = crate / "src" / "functional.rs"
        write(declaration, declaration_text)
        write(
            crate / "src" / "lib.rs",
            "// DiagnosticBundle and ExportDiagnosticReport are planned here.\n"
            "pub fn noop() {}\n",
        )
        findings = guard.analyze_frontend("demo", crate, declaration, root)
        assert findings == [], findings


def test_baseline_suppression_and_template(guard) -> None:
    finding = guard.Finding(
        frontend="tui",
        intent="DiagnosticBundle",
        kind=guard.UNSUPPORTED_BUT_IMPLEMENTED,
        decision="Unsupported",
        detail="synthetic",
    )
    other = guard.Finding(
        frontend="tui",
        intent="CurrentWindowScreenshot",
        kind=guard.UNSUPPORTED_BUT_IMPLEMENTED,
        decision="Unsupported",
        detail="synthetic",
    )
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        baseline_path = Path(raw) / "baseline.json"
        write(
            baseline_path,
            json.dumps(
                {
                    "entries": [
                        {
                            "frontend": "tui",
                            "intent": "DiagnosticBundle",
                            "kind": "unsupported_but_implemented",
                            "reason": "known drift",
                            "remove_when": "declaration is corrected",
                        }
                    ]
                }
            ),
        )
        baseline = guard.load_baseline(baseline_path)
        assert baseline.match(finding) is not None
        assert baseline.match(other) is None
        template = json.loads(guard.baseline_template([finding, other]))
        assert len(template["entries"]) == 2
        assert template["entries"][0]["intent"] == "DiagnosticBundle"
        assert "remove_when" in template["entries"][0]


def _build_end_to_end_repo(root: Path) -> None:
    """Create the four real frontend paths, with one TUI contradiction."""

    compliant = {
        "gpui": (
            "crates/taskmanager-gpui",
            "crates/taskmanager-gpui/src/gpui_app/functional.rs",
            "dashboard.active-alerts",
            "Reference",
        ),
        "iced": (
            "crates/taskmanager-iced",
            "crates/taskmanager-iced/src/functional.rs",
            "alerts.page.active",
            "Local",
        ),
        "bevy": (
            "crates/taskmanager-bevy-ui",
            "crates/taskmanager-bevy-ui/src/functional.rs",
            "alerts.page.active",
            "Local",
        ),
    }
    for _, (crate_rel, decl_rel, route, kind) in compliant.items():
        write(
            root / decl_rel,
            "const fn decision(intent: ProductIntent) -> SurfaceDecision {\n"
            "    match intent {\n"
            "        ProductIntent::ActiveAlerts => SurfaceDecision::"
            f'{kind} {{ route: "{route}" }},\n'
            "    }\n"
            "}\n",
        )
        write(root / crate_rel / "src" / "lib.rs", f'pub const ROUTE: &str = "{route}";\n')

    write(
        root / "crates/taskmanager-tui/src/functional.rs",
        "const fn decision(intent: ProductIntent) -> SurfaceDecision {\n"
        "    match intent {\n"
        "        ProductIntent::DiagnosticBundle => SurfaceDecision::Unsupported {\n"
        '            reason: "no diagnostic surface",\n'
        "        },\n"
        "    }\n"
        "}\n",
    )
    write(
        root / "crates/taskmanager-tui/src/diagnostic_report.rs",
        "use taskmanager_core::DiagnosticBundlePlan;\n",
    )


def test_end_to_end_exit_codes(guard) -> None:
    with tempfile.TemporaryDirectory(prefix="parity-selftest-") as raw:
        root = Path(raw)
        _build_end_to_end_repo(root)

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = guard.main(["--root", str(root), "--json"])
        assert code == 1, code
        payload = json.loads(out.getvalue())
        assert len(payload["findings"]) == 1, payload
        assert payload["findings"][0]["kind"] == guard.UNSUPPORTED_BUT_IMPLEMENTED

        baseline_path = root / "baseline.json"
        write(
            baseline_path,
            json.dumps(
                {
                    "entries": [
                        {
                            "frontend": "tui",
                            "intent": "DiagnosticBundle",
                            "kind": "unsupported_but_implemented",
                            "reason": "known drift",
                            "remove_when": "declaration corrected",
                        }
                    ]
                }
            ),
        )
        with quiet():
            suppressed_code = guard.main(
                ["--root", str(root), "--baseline", str(baseline_path)]
            )
        assert suppressed_code == 0, suppressed_code

        stale_path = root / "stale.json"
        write(
            stale_path,
            json.dumps(
                {
                    "entries": [
                        {
                            "frontend": "tui",
                            "intent": "DiagnosticBundle",
                            "kind": "unsupported_but_implemented",
                            "reason": "known drift",
                            "remove_when": "declaration corrected",
                        },
                        {
                            "frontend": "gpui",
                            "intent": "DiagnosticBundle",
                            "kind": "unsupported_but_implemented",
                            "reason": "no longer applies",
                            "remove_when": "n/a",
                        },
                    ]
                }
            ),
        )
        with quiet():
            lenient_code = guard.main(
                ["--root", str(root), "--baseline", str(stale_path)]
            )
            stale_code = guard.main(
                [
                    "--root",
                    str(root),
                    "--strict-baseline",
                    "--baseline",
                    str(stale_path),
                ]
            )
        assert lenient_code == 0, lenient_code
        assert stale_code == 1, stale_code


def main() -> int:
    guard = load_guard()
    tests = [
        test_parser_handles_both_shapes,
        test_compliant_sample_passes,
        test_unsupported_but_implemented_is_reported,
        test_unsupported_alias_evidence_is_reported,
        test_route_missing_is_reported,
        test_route_pinned_in_crate_test_counts,
        test_comments_are_not_evidence,
        test_baseline_suppression_and_template,
        test_end_to_end_exit_codes,
    ]
    for test in tests:
        test(guard)
        print(f"ok  {test.__name__}")
    print(f"parity-declaration-consistency self-test: ok ({len(tests)} cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
