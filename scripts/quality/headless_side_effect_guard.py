#!/usr/bin/env python3
"""Guard: automated tests must not produce OS-visible side effects.

The repository standard forbids automated tests from dumping anything into
the user's system: no real windows, dialogs, notifications, tray items, URL
openers, privilege prompts, or interactive binaries. GUI/capture evidence is
only produced by explicit opt-in flows (``--with-gui`` / capture scripts),
never by ``cargo nextest``.

This guard flags the mechanical side-effect patterns in test files:

  - spawning interactive or privileged binaries through ``Command::new``
    (pkexec, sudo, xdg-open, notify-send, zenity, kdialog, gio, open, start,
    explorer, yad, xmessage, Xdialog, plasmashell, gnome-shell);
  - calling native dialog APIs (MessageBox, TaskDialog, NSAlert) or the
    `notify-rust` toast builder from a test;
  - starting a real toolkit event loop or terminal inside a test
    (gpui::Application::new / Application::run / iced::application /
    MainLoop / Terminal::new(std::io::stdout()));
  - constructing the real production host from a test
    (`NativeAppHost::production()` resolves real user config/history paths);
  - using the system temp directory (`std::env::temp_dir()`); test scratch
    must live under the repository `.tmp/` via `crate::repo_temp_dir()`.

String literals that merely *mention* these names (error fixtures, policy
text) are not violations; only actual spawn/run call sites are.

``--mode report`` lists violations without failing; ``--mode enforce`` turns
the list into a hard failure.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

SPAWN_BINARIES = re.compile(
    r'Command::new\(\s*"('
    r"pkexec|sudo|xdg-open|notify-send|zenity|kdialog|gio\s+open|open|start|explorer"
    r"|yad|xmessage|Xdialog|plasmashell|gnome-shell"
    r')"\s*\)'
)
NATIVE_DIALOG = re.compile(
    r"\b(MessageBox|TaskDialog|NSAlert)\b|notify_rust::Notification::new\("
)
REAL_USER_PATHS = re.compile(r"NativeAppHost::production\(")
EVENT_LOOP = re.compile(
    r"(gpui::)?Application::new\(|Application::run\(|iced::application\(|MainLoop::new\("
)
REAL_TERMINAL = re.compile(
    r"Terminal::new\(\s*std::io::(stdout|stderr)\(\)\s*\)"
)
SYSTEM_TEMP = re.compile(r"std::env::temp_dir\(\)")


@dataclass(frozen=True)
class Violation:
    path: str
    line: int
    code: str


def scan_file(path: Path, repository: Path) -> list[Violation]:
    violations: list[Violation] = []
    for index, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        if (
            SPAWN_BINARIES.search(line)
            or NATIVE_DIALOG.search(line)
            or REAL_USER_PATHS.search(line)
            or EVENT_LOOP.search(line)
            or REAL_TERMINAL.search(line)
            or SYSTEM_TEMP.search(line)
        ):
            violations.append(
                Violation(
                    path=path.relative_to(repository).as_posix(),
                    line=index,
                    code=line.strip(),
                )
            )
    return violations


def test_files(repository: Path) -> list[Path]:
    roots: list[Path] = [repository / "tests"]
    crates = repository / "crates"
    if crates.is_dir():
        roots.extend(
            package / "tests"
            for package in sorted(crates.iterdir())
            if (package / "Cargo.toml").is_file() and (package / "tests").is_dir()
        )
    files: list[Path] = []
    for root in roots:
        if root.is_dir():
            files.extend(sorted(root.rglob("*.rs")))
    return files


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="headless-side-effect-gate-") as raw:
        repository = Path(raw)
        tests = repository / "tests" / "logic"
        tests.mkdir(parents=True)
        cases = [
            (
                "spawn_ui_binary.rs",
                'let _ = std::process::Command::new("xdg-open").arg(url).status();\n',
                1,
            ),
            (
                "spawn_privileged.rs",
                'let _ = std::process::Command::new("pkexec").status();\n',
                1,
            ),
            (
                "spawn_desktop_shell.rs",
                'let _ = std::process::Command::new("plasmashell").arg("--version").status();\n',
                1,
            ),
            (
                "native_dialog.rs",
                "let _ = MessageBox::new();\n",
                1,
            ),
            (
                "toast.rs",
                "notify_rust::Notification::new().summary(\"x\").show();\n",
                1,
            ),
            (
                "real_user_paths.rs",
                "let host = taskmanager_app_host::NativeAppHost::production();\n",
                1,
            ),
            (
                "event_loop.rs",
                "gpui::Application::new().run(|cx| {});\n",
                1,
            ),
            (
                "real_terminal.rs",
                "let mut terminal = ratatui::Terminal::new(std::io::stdout()).unwrap();\n",
                1,
            ),
            (
                "mention_only.rs",
                'assert!(script.contains("xdg-open"));\n',
                0,
            ),
            (
                "headless_behavior.rs",
                "let text = render();\nassert!(text.contains(\"CPU\"));\n",
                0,
            ),
            (
                "system_temp.rs",
                "let root = std::env::temp_dir().join(\"scratch\");\n",
                1,
            ),
        ]
        for name, content, expected in cases:
            path = tests / name
            path.write_text(content, encoding="utf-8")
            actual = len(scan_file(path, repository))
            if actual != expected:
                print(f"self-test FAIL {name}: expected {expected}, got {actual}")
                failures += 1
    return failures


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode", choices=("report", "enforce"), default="report"
    )
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return 1 if self_test() else 0

    repository = args.repo_root.resolve()
    violations: list[Violation] = []
    for path in test_files(repository):
        violations.extend(scan_file(path, repository))

    for violation in violations:
        print(
            f"{violation.path}:{violation.line}: "
            f"OS-visible test side effect: {violation.code}"
        )

    print(f"OS-visible side-effect violations: {len(violations)}")
    if args.mode == "enforce" and violations:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
