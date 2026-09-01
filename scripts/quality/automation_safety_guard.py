#!/usr/bin/env python3
"""Reject automation patterns that can hang or kill unrelated processes."""

from __future__ import annotations

import argparse
import ast
import re
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    code: str
    message: str


class MutationCollector(ast.NodeVisitor):
    """Collect directly assigned names without crossing a nested scope."""

    def __init__(self) -> None:
        self.names: set[str] = set()

    def visit_Name(self, node: ast.Name) -> None:
        if isinstance(node.ctx, (ast.Store, ast.Del)):
            self.names.add(node.id)

    def visit_FunctionDef(self, _node: ast.FunctionDef) -> None:
        return

    def visit_AsyncFunctionDef(self, _node: ast.AsyncFunctionDef) -> None:
        return

    def visit_ClassDef(self, _node: ast.ClassDef) -> None:
        return

    def visit_Lambda(self, _node: ast.Lambda) -> None:
        return


def qualified_name(node: ast.expr) -> str | None:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        owner = qualified_name(node.value)
        return f"{owner}.{node.attr}" if owner else None
    return None


def condition_names(node: ast.expr) -> set[str]:
    return {
        child.id
        for child in ast.walk(node)
        if isinstance(child, ast.Name) and isinstance(child.ctx, ast.Load)
    }


def mutation_names(statements: list[ast.stmt]) -> set[str]:
    collector = MutationCollector()
    for statement in statements:
        collector.visit(statement)
    return collector.names


def validate_python_text(path: str, source: str) -> list[Finding]:
    findings: list[Finding] = []
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError as error:
        return [Finding(path, error.lineno or 1, "AUTO000", f"invalid Python: {error.msg}")]

    for node in ast.walk(tree):
        if isinstance(node, ast.While):
            tested = condition_names(node.test)
            advanced = mutation_names(node.body)
            if not tested or tested.isdisjoint(advanced):
                findings.append(
                    Finding(
                        path,
                        node.lineno,
                        "AUTO001",
                        "while loop has no syntactically provable condition-variable progress; use a bounded iterator",
                    )
                )
        elif isinstance(node, ast.Call):
            name = qualified_name(node.func)
            if name in {
                "subprocess.run",
                "subprocess.call",
                "subprocess.check_call",
                "subprocess.check_output",
            }:
                timeout = next((item.value for item in node.keywords if item.arg == "timeout"), None)
                if timeout is None or isinstance(timeout, ast.Constant) and timeout.value is None:
                    findings.append(
                        Finding(path, node.lineno, "AUTO002", f"{name} requires a finite timeout=")
                    )
            elif name == "subprocess.Popen":
                starts_new_session = next(
                    (
                        item.value
                        for item in node.keywords
                        if item.arg == "start_new_session"
                    ),
                    None,
                )
                if not isinstance(starts_new_session, ast.Constant) or starts_new_session.value is not True:
                    findings.append(
                        Finding(
                            path,
                            node.lineno,
                            "AUTO003",
                            "subprocess.Popen requires start_new_session=True and explicit lifecycle ownership",
                        )
                    )
            elif name in {"os.system", "os.popen"}:
                findings.append(
                    Finding(
                        path,
                        node.lineno,
                        "AUTO003",
                        f"{name} is unmanaged; use subprocess.run(..., timeout=..., check=True)",
                    )
                )
        elif isinstance(node, ast.ImportFrom) and node.module == "subprocess":
            findings.append(
                Finding(
                    path,
                    node.lineno,
                    "AUTO004",
                    "import subprocess as a module so timeout calls remain statically visible",
                )
            )
        elif isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name == "subprocess" and alias.asname is not None:
                    findings.append(
                        Finding(path, node.lineno, "AUTO004", "do not alias the subprocess module")
                    )
    return findings


BACKGROUND_OPERATOR = re.compile(r"(?<![&>])&(?![&>])")
INLINE_PYTHON = re.compile(r"\bpython(?:3)?\s+-\s*<<")
PYTHON_COMMAND = re.compile(r"\bpython3\b")
GLOBAL_KILL = re.compile(r"\b(?:pkill|killall)\b")


def validate_shell_text(path: str, source: str) -> list[Finding]:
    findings: list[Finding] = []
    active_lines = [
        (number, line)
        for number, line in enumerate(source.splitlines(), start=1)
        if line.strip() and not line.lstrip().startswith("#")
    ]
    background_lines = [
        number for number, line in active_lines if BACKGROUND_OPERATOR.search(line)
    ]
    has_exit_trap = bool(re.search(r"\btrap\b[^\n]*\bEXIT\b", source))
    has_child_capture = "$!" in source
    if background_lines and not has_exit_trap:
        findings.append(
            Finding(path, background_lines[0], "AUTO101", "background process requires an EXIT trap")
        )
    if background_lines and not has_child_capture:
        findings.append(
            Finding(path, background_lines[0], "AUTO102", "capture every background child with $!")
        )

    for number, line in active_lines:
        if INLINE_PYTHON.search(line):
            findings.append(
                Finding(path, number, "AUTO103", "inline Python heredocs are forbidden; commit a tested script")
            )
        if GLOBAL_KILL.search(line):
            findings.append(
                Finding(path, number, "AUTO104", "global name-based process killing is forbidden; use an exact PID/PGID")
            )
        match = PYTHON_COMMAND.search(line)
        if (
            match
            and "timeout" not in line[: match.start()]
            and "command -v python3" not in line
            and "command_name" not in line
        ):
            findings.append(
                Finding(path, number, "AUTO105", "shell-launched Python requires an external timeout")
            )
    return findings


def scan(repository: Path) -> tuple[int, list[Finding]]:
    scripts = repository / "scripts"
    paths = sorted(scripts.rglob("*.py")) + sorted(scripts.rglob("*.sh"))
    findings: list[Finding] = []
    for path in paths:
        source = path.read_text(encoding="utf-8")
        relative = path.relative_to(repository).as_posix()
        if path.suffix == ".py":
            findings.extend(validate_python_text(relative, source))
        else:
            findings.extend(validate_shell_text(relative, source))
    return len(paths), sorted(findings)


def self_test() -> None:
    runaway = """\
probe = start
while probe != root and probe != probe.parent:
    if (probe / "Cargo.toml").is_file():
        break
"""
    bounded = """\
for probe in path.parents:
    if probe == root:
        break
"""
    advancing = """\
offset = 0
while offset < len(data):
    offset += 1
"""
    assert {item.code for item in validate_python_text("runaway.py", runaway)} == {"AUTO001"}
    assert not validate_python_text("bounded.py", bounded)
    assert not validate_python_text("advancing.py", advancing)
    assert {item.code for item in validate_python_text(
        "child.py", "import subprocess\nsubprocess.run(['tool'])\n"
    )} == {"AUTO002"}
    assert not validate_python_text(
        "child.py", "import subprocess\nsubprocess.run(['tool'], timeout=10, check=True)\n"
    )
    assert not validate_python_text(
        "supervisor.py",
        "import subprocess\nsubprocess.Popen(['tool'], start_new_session=True)\n",
    )
    assert {item.code for item in validate_python_text(
        "unsafe-supervisor.py", "import subprocess\nsubprocess.Popen(['tool'])\n"
    )} == {"AUTO003"}
    unsafe_shell = "python3 - <<'PY'\nPY\nworker &\npkill worker\n"
    assert {item.code for item in validate_shell_text("unsafe.sh", unsafe_shell)} == {
        "AUTO101",
        "AUTO102",
        "AUTO103",
        "AUTO104",
        "AUTO105",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Automation safety guard self-test: PASS")
        return 0
    scanned, findings = scan(args.repo_root.resolve())
    for finding in findings:
        print(f"{finding.path}:{finding.line}: {finding.code} {finding.message}")
    print(f"Automation safety guard: scanned={scanned} findings={len(findings)}")
    return int(bool(findings))


if __name__ == "__main__":
    sys.exit(main())
