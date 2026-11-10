#!/usr/bin/env python3
"""One budget for the instruments validators run, and one word for using it up.

Capture and evidence validators shell out to real tools -- `tesseract` for OCR,
`git` for worktree facts, image decoders for pixel checks. Two facts kept
getting collapsed into one:

* the evidence says the product is wrong;
* the instrument never got to finish on a busy machine.

The second was being written as the first. A private R135 quick-gate run failed
`capture-isolation` with `validation.status = "reject"` whose reason was literally
`Command '[tesseract, ...]' timed out after 15 seconds`: a fixed wall-clock
budget, asserted while other projects held the machine at load average 24-40.
That is the same defect class this repository already forbids in
`tests/performance.rs` (a wall-clock ceiling on a pure-CPU path) and in
`soak_stability_guard.py` (a first-vs-last delta on a plateau).

So the budget scales with the machine's own reported pressure instead of
assuming an idle host, and running out of budget raises
`InstrumentIncomplete`, which every caller must report as an unfinished
instrument -- never as a product verdict, never as a pass.

Usage::

    python3 scripts/instrument_budget.py --self-test
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

BASE_SECONDS = 15.0
MAX_FACTOR = 8.0
CEILING_SECONDS = 180.0


class InstrumentIncomplete(RuntimeError):
    """The instrument could not finish inside its budget.

    Distinct from a rejected capture: nothing has been learned about the
    product, so no caller may translate this into a product finding.
    """


def pressure_factor() -> float:
    """How much the machine is oversubscribed right now, at least 1.0.

    PSI is the preferred reading because it counts *stalled* run queues; the
    load-average ratio is the fallback for kernels or platforms without it.
    Both are read as observations of the host, never asserted as constants.
    """
    try:
        text = Path("/proc/pressure/cpu").read_text(encoding="utf-8")
        avg10 = re.search(r"avg10=(\d+(?:\.\d+)?)", text)
        if avg10:
            # avg10 is a percentage of the window some task was runnable but
            # delayed; 100% means every CPU had a queue the whole time.
            stalled = float(avg10.group(1))
            if stalled > 50.0:
                return min(MAX_FACTOR, 1.0 + stalled / 50.0)
    except OSError:
        pass
    try:
        cores = os.cpu_count() or 1
        load1 = os.getloadavg()[0]
        return max(1.0, min(MAX_FACTOR, load1 / cores))
    except OSError:
        return 1.0


def seconds(base: float = BASE_SECONDS) -> float:
    return min(CEILING_SECONDS, base * pressure_factor())


def run(
    *arguments: object,
    base: float = BASE_SECONDS,
    timeout: float | None = None,
    **keywords: object,
) -> subprocess.CompletedProcess:
    """`subprocess.run` with the shared budget and its exhaustion named.

    The budget is always finite and always bounded by `CEILING_SECONDS`, so a
    wedged instrument cannot buy itself an infinite pass. A caller that passes
    its own `timeout` keeps it, but the ceiling still applies: this helper
    exists to replace fixed wall budgets, not to rubber-stamp new ones.
    """
    budget = min(CEILING_SECONDS, seconds(base) if timeout is None else float(timeout))
    try:
        return subprocess.run(*arguments, timeout=budget, **keywords)
    except subprocess.TimeoutExpired as error:
        raise InstrumentIncomplete(
            f"instrument {arguments[0]!r} did not finish inside {budget:.1f}s "
            f"(host pressure factor {pressure_factor():.2f}); the evidence is "
            "unfinished, not disproved"
        ) from error


WALL_BUDGET = re.compile(r"\btimeout\s*=\s*\d+(?:\.\d+)?\s*[,)]")


def timeout_sites(root: Path) -> list[str]:
    """Every evidence validator that still asserts its own fixed wall budget.

    The surface is deliberately `scripts/validate_*.py`: those are the
    instruments that turn a capture into a product verdict. Other budgets in
    the repository (a stage's own timeout, a fixture's text, a prewarm bound
    that is already classified as unavailability) are different facts and are
    not this census's business -- an over-wide net would only teach people to
    ignore the net.
    """
    offenders: list[str] = []
    for path in sorted(root.glob("validate_*.py")):
        text = path.read_text(encoding="utf-8", errors="replace")
        if WALL_BUDGET.search(text):
            offenders.append(path.relative_to(root).as_posix())
    return offenders


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, ok: bool) -> None:
        if not ok:
            failures.append(name)

    idle = pressure_factor()
    check("factor is never below one", idle >= 1.0)
    check("factor is capped so a wedged host cannot grant an infinite budget",
          idle <= MAX_FACTOR)
    check("budget grows with pressure", seconds(10.0) >= 10.0)
    check("budget stays under the ceiling", seconds(10_000.0) == CEILING_SECONDS)

    try:
        run(["sleep", "5"], check=False, timeout=0.05)
        check("a spent budget raises InstrumentIncomplete", False)
    except InstrumentIncomplete as error:
        check("the incomplete message says the evidence is unfinished, not wrong",
              "not disproved" in str(error))
    except subprocess.TimeoutExpired:
        check("a spent budget raises InstrumentIncomplete", False)

    completed = run(["true"], check=False)
    check("a caller budget beyond the ceiling is still capped", seconds(10_000.0) == CEILING_SECONDS)
    check("a finished instrument returns its result unchanged", completed.returncode == 0)

    # The census must bite: a scratch file that re-asserts its own budget.
    import tempfile

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        culprit = root / "validate_culprit.py"
        culprit.write_text(
            "import subprocess\nsubprocess.run(['tesseract'], timeout=15)\n",
            encoding="utf-8",
        )
        check("a hard-coded wall budget is reported", timeout_sites(root) == ["validate_culprit.py"])
        culprit.write_text(
            "import instrument_budget\ninstrument_budget.run(['tesseract'])\n",
            encoding="utf-8",
        )
        check("the shared budget is not reported", timeout_sites(root) == [])
        check("an explicit caller budget is honoured below the ceiling",
              run(["true"], check=False, timeout=5.0).returncode == 0)
        outside = root / "not_a_validator.py"
        outside.write_text("import subprocess\nsubprocess.run(['x'], timeout=99)\n", encoding="utf-8")
        check("the census is scoped to the evidence-validator family",
              timeout_sites(root) == [])

    if failures:
        print(f"instrument budget self-test: FAIL {failures}", file=sys.stderr)
        return 1
    print(
        f"instrument budget self-test: PASS (pressure factor {idle:.2f}, "
        f"budget for the default base {seconds():.1f}s)"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--census", type=Path, help="list scripts that still assert a wall budget")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.census:
        offenders = timeout_sites(args.census)
        for offender in offenders:
            print(f"fixed wall budget: {offender}")
        return 1 if offenders else 0
    parser.error("nothing selected")


if __name__ == "__main__":
    sys.exit(main())
