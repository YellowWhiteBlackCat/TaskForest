#!/usr/bin/env python3
"""Validate the allowlisted TaskForest install inventory and source wiring.

The TSV keeps exactly one authoritative row per destination. A destination may
be provided by several packages/recipes (for example the shared hicolor icon or
the MSI components common to every Windows product): `install_method` then lists
every provider token joined by " or ", and the guard requires each packaging
recipe's destination to credit that recipe. Windows MSI landing spots are
expressed with WiX standard-directory tokens and HKLM registry keys instead of a
Linux `/usr` prefix.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


REQUIRED_COLUMNS = (
    "id",
    "scope",
    "lifecycle",
    "destination",
    "artifact",
    "owner",
    "install_method",
    "purpose",
    "privilege",
    "conflict_policy",
    "removal_method",
    "source",
)
ALLOWED_LIFECYCLES = {
    "package-managed",
    "optional-root",
    "approved-pending",
    "developer-user",
    "user-managed",
}
PATH_ANNOTATION = re.compile(
    r"<annotate key=\"org\.freedesktop\.policykit\.exec\.path\">([^<]+)</annotate>"
)
PKG_DESTINATION = re.compile(r"\$pkgdir(/[^\s\"]+)")
DEB_INSTALL = re.compile(r"^\s*(?:install|ln)\b")
DEB_DESTINATION = re.compile(r'"\$work(/usr/[^"]*)"')
DEB_LADDER_LOOP = re.compile(r"for\s+size\s+in\s+([0-9 ]+);\s*do")
DEB_LADDER_SIZE = re.compile(r"\$\{size\}")
WIX_UI_ARM = re.compile(
    r'exe_name="taskforest-([gitb])\.exe".*?shortcut_name="TaskForest([GITB])"',
    re.S,
)
MSI_PROVIDER = "packaging/windows/build-msi.sh ui={ui}"
WIX_PLACEHOLDER = "$("
SETUP_RULE = re.compile(r'const RULE_PATH: &str = "([^\"]+)";')
MANAGER_DESTINATIONS = (
    "/usr/libexec/taskforest-privilege-helper",
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.perf-helper.policy",
    "/usr/libexec/taskforest-net-launcher",
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy",
    "/usr/libexec/taskforest-process-control-helper",
    "/usr/share/polkit-1/actions/io.github.YellowWhiteBlackCat.TaskForest.process-control.policy",
)
PROVIDER_SEPARATOR = " or "
COMMON_DEB_PROVIDER = "packaging/debian/build-deb-common.sh"
COMMON_RPM_PROVIDER = "packaging/rpm/taskforest-common.spec"
COMMON_PACKAGE = "taskforest-common"
# UI product packages that consume the shared data package: the three that
# render a desktop entry and therefore the shared hicolor icon set. TUI ships
# no shared desktop asset and intentionally carries no such dependency.
COMMON_CONSUMER_CONTROLS = ("control", "control-iced", "control-bevy")
COMMON_CONSUMER_SPECS = ("taskforest.spec", "taskforest-i.spec", "taskforest-b.spec")


def load_rows(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if tuple(reader.fieldnames or ()) != REQUIRED_COLUMNS:
            raise ValueError("manifest header does not match the required schema")
        rows = list(reader)
    if not rows:
        raise ValueError("manifest has no rows")
    return rows


def credit_tokens(method: str) -> set[str]:
    """Split a provider list joined by " or " into its individual tokens."""

    return {token.strip() for token in method.split(PROVIDER_SEPARATOR) if token.strip()}


def shell_logical_lines(source: str) -> list[str]:
    """Join backslash continuations so one install command is one record."""

    records: list[str] = []
    pending: list[str] = []
    for line in source.splitlines():
        stripped = line.rstrip()
        pending.append(stripped)
        if not stripped.endswith("\\"):
            records.append(" ".join(pending))
            pending = []
    if pending:
        records.append(" ".join(pending))
    return records


def deb_install_destinations(source: str) -> set[str]:
    """Return every $work/usr destination written by a standalone deb recipe."""

    sizes = [
        size
        for match in DEB_LADDER_LOOP.finditer(source)
        for size in match.group(1).split()
    ]
    destinations: set[str] = set()
    for line in shell_logical_lines(source):
        if not DEB_INSTALL.match(line):
            continue
        for destination in DEB_DESTINATION.findall(line):
            if DEB_LADDER_SIZE.search(destination):
                destinations.update(
                    DEB_LADDER_SIZE.sub(size, destination) for size in sizes
                )
            else:
                destinations.add(destination)
    return destinations


def rpm_file_destinations(source: str) -> set[str]:
    """Return the absolute %files paths of an RPM spec (never %dir entries)."""

    return {
        stripped
        for line in source.splitlines()
        if (stripped := line.strip()).startswith("/usr/")
    }


def wix_destinations(source: str) -> tuple[set[str], set[str]]:
    """Return (shared, per-product) destinations declared by a WiX manifest.

    Paths keep the WiX standard-directory token, the Windows separator, and any
    ``$(var.*)`` placeholder the build wrapper substitutes per UI target.
    """

    tree = ET.fromstring(source)
    shared: set[str] = set()
    per_product: set[str] = set()

    def walk(element: ET.Element, path: str) -> None:
        for child in element:
            tag = child.tag.rpartition("}")[2]
            if tag == "StandardDirectory":
                walk(child, f"[{child.get('Id')}]")
            elif tag == "Directory":
                walk(child, f"{path}\\{child.get('Name')}")
            elif tag == "File":
                source_path = child.get("Source", "")
                name = child.get("Name") or source_path.rsplit("/", 1)[-1]
                bucket = per_product if WIX_PLACEHOLDER in name else shared
                bucket.add(f"{path}\\{name}")
            elif tag == "Shortcut":
                name = child.get("Name", "")
                bucket = per_product if WIX_PLACEHOLDER in name else shared
                bucket.add(f"{path}\\{name}.lnk")
            elif tag == "RegistryValue":
                shared.add(
                    f"{child.get('Root')}\\{child.get('Key')}\\{child.get('Name')}"
                )
            else:
                walk(child, path)

    walk(tree, "")
    return shared, per_product


def msi_destinations(root: Path) -> list[tuple[str, str]]:
    """Pair every MSI landing spot with the provider token that installs it."""

    manifest = root / "packaging/windows/taskforest.wxs"
    build = root / "packaging/windows/build-msi.sh"
    shared, per_product = wix_destinations(manifest.read_text(encoding="utf-8"))
    arms = WIX_UI_ARM.findall(build.read_text(encoding="utf-8"))
    pairs: list[tuple[str, str]] = []
    for exe_letter, shortcut_letter in arms:
        token = MSI_PROVIDER.format(ui=exe_letter.upper())
        replacements = {
            "$(var.ExeName)": f"taskforest-{exe_letter}.exe",
            "$(var.ShortcutName)": f"TaskForest{shortcut_letter}",
        }
        for destination in per_product:
            resolved = destination
            for placeholder, value in replacements.items():
                resolved = resolved.replace(placeholder, value)
            pairs.append((token, resolved))
    for destination in shared:
        for exe_letter, _ in arms:
            pairs.append((MSI_PROVIDER.format(ui=exe_letter.upper()), destination))
    return pairs


def shared_owner_findings(
    provider_destinations: dict[str, set[str]], common_provider: str
) -> list[str]:
    """Reject a shared destination installed by more than the common owner.

    After the taskforest-common split the common data package is the single
    owner of each shared DEB/RPM path. Any other recipe in the same format that
    still installs one of those paths is a regression to the multi-provider
    overflow the split exists to remove.
    """

    findings: list[str] = []
    common = provider_destinations.get(common_provider)
    if not common:
        findings.append(f"common provider installs no destinations: {common_provider}")
        return findings
    for provider, destinations in provider_destinations.items():
        if provider == common_provider:
            continue
        for destination in sorted(destinations & common):
            findings.append(
                f"shared destination still installed by non-common provider "
                f"{provider!r}: {destination}"
            )
    return findings


def declared_dependency_findings(
    control_texts: dict[str, str], spec_texts: dict[str, str]
) -> list[str]:
    """Require every shared-asset consumer to declare the common dependency."""

    findings: list[str] = []
    dependency = re.compile(
        r"^Depends:.*\b" + re.escape(COMMON_PACKAGE) + r"\b", re.M
    )
    requirement = re.compile(
        r"^Requires:.*\b" + re.escape(COMMON_PACKAGE) + r"\b", re.M
    )
    for name in COMMON_CONSUMER_CONTROLS:
        text = control_texts.get(name)
        if text is None:
            findings.append(f"missing control template: packaging/debian/{name}")
        elif not dependency.search(text):
            findings.append(
                f"packaging/debian/{name} does not depend on {COMMON_PACKAGE}"
            )
    for name in COMMON_CONSUMER_SPECS:
        text = spec_texts.get(name)
        if text is None:
            findings.append(f"missing rpm spec: packaging/rpm/{name}")
        elif not requirement.search(text):
            findings.append(
                f"packaging/rpm/{name} does not require {COMMON_PACKAGE}"
            )
    return findings


def validate_rows(rows: list[dict[str, str]]) -> list[str]:
    findings: list[str] = []
    ids: set[str] = set()
    destinations: set[str] = set()
    for row in rows:
        row_id = row["id"]
        destination = row["destination"]
        if not row_id or row_id in ids:
            findings.append(f"duplicate/empty id: {row_id!r}")
        if not destination or destination in destinations:
            findings.append(f"duplicate/empty destination: {destination!r}")
        if row["lifecycle"] not in ALLOWED_LIFECYCLES:
            findings.append(f"unsupported lifecycle for {row_id}: {row['lifecycle']!r}")
        for field in REQUIRED_COLUMNS:
            if not row[field].strip():
                findings.append(f"empty {field} for {row_id}")
        if any(
            not token
            for token in row["install_method"].split(PROVIDER_SEPARATOR)
        ):
            findings.append(f"install_method has an empty provider token for {row_id}")
        ids.add(row_id)
        destinations.add(destination)
    return findings


def validate_wiring(root: Path, rows: list[dict[str, str]]) -> list[str]:
    findings: list[str] = []
    by_destination = {row["destination"]: row for row in rows}

    def require(provider: str, destination: str, label: str) -> None:
        row = by_destination.get(destination)
        if row is None:
            findings.append(f"{label} destination missing from manifest: {destination}")
        elif provider not in credit_tokens(row["install_method"]):
            findings.append(
                f"{label} destination does not credit provider {provider!r}: {destination}"
            )

    for policy in sorted((root / "polkit").glob("*.policy.in")):
        text = policy.read_text(encoding="utf-8")
        for destination in PATH_ANNOTATION.findall(text):
            if destination not in by_destination:
                findings.append(f"policy destination missing from manifest: {policy}: {destination}")

    setup = root / "crates/taskmanager-setup-helper/src/main.rs"
    setup_match = SETUP_RULE.search(setup.read_text(encoding="utf-8"))
    if setup_match and setup_match.group(1) not in by_destination:
        findings.append(f"setup helper destination missing from manifest: {setup_match.group(1)}")

    pkgbuild = root / "packaging/arch/PKGBUILD"
    for destination in PKG_DESTINATION.findall(pkgbuild.read_text(encoding="utf-8")):
        destination = destination.replace("$pkgname", "taskforest-git")
        require(pkgbuild.relative_to(root).as_posix(), destination, "PKGBUILD")

    deb_destinations: dict[str, set[str]] = {}
    for script in sorted((root / "packaging/debian").glob("build-deb-*.sh")):
        provider = script.relative_to(root).as_posix()
        deb_destinations[provider] = deb_install_destinations(
            script.read_text(encoding="utf-8")
        )
        for destination in sorted(deb_destinations[provider]):
            require(provider, destination, f"deb recipe {script.name}")
    findings.extend(shared_owner_findings(deb_destinations, COMMON_DEB_PROVIDER))

    rpm_destinations: dict[str, set[str]] = {}
    for spec in sorted((root / "packaging/rpm").glob("*.spec")):
        provider = spec.relative_to(root).as_posix()
        rpm_destinations[provider] = rpm_file_destinations(
            spec.read_text(encoding="utf-8")
        )
        for destination in sorted(rpm_destinations[provider]):
            require(provider, destination, f"rpm spec {spec.name}")
    findings.extend(shared_owner_findings(rpm_destinations, COMMON_RPM_PROVIDER))

    # Shared-asset consumers must take the dependency instead of re-shipping
    # the path. The consumer set is explicit: those are the packages whose own
    # recipes no longer install the common destinations.
    control_texts = {
        name: (root / "packaging/debian" / name).read_text(encoding="utf-8")
        for name in COMMON_CONSUMER_CONTROLS
        if (root / "packaging/debian" / name).is_file()
    }
    spec_texts = {
        name: (root / "packaging/rpm" / name).read_text(encoding="utf-8")
        for name in COMMON_CONSUMER_SPECS
        if (root / "packaging/rpm" / name).is_file()
    }
    findings.extend(declared_dependency_findings(control_texts, spec_texts))

    msi_pairs = msi_destinations(root)
    if not msi_pairs:
        findings.append("MSI manifest produced no destinations to verify")
    for provider, destination in msi_pairs:
        require(provider, destination, "MSI")

    manager = root / "scripts/manage-polkit-install.sh"
    manager_text = manager.read_text(encoding="utf-8")
    for destination in MANAGER_DESTINATIONS:
        if destination not in manager_text or destination not in by_destination:
            findings.append(f"polkit manager/manifest wiring missing: {destination}")

    required_ids = {
        "DEV-FRONTENDS-STATE",
        "DEV-GPUI-DESKTOP",
        "DEV-ICED-DESKTOP",
        "DEV-ICON",
        "DEV-HICOLOR-INDEX",
    }
    ids = {row["id"] for row in rows}
    for row_id in sorted(required_ids - ids):
        findings.append(f"developer install row missing: {row_id}")

    for row in (row for row in rows if row["lifecycle"] == "developer-user"):
        authority = row["source"]
        if not authority.startswith("scripts/") or not (root / authority).is_file():
            findings.append(f"developer install authority is not an existing script: {row['id']}: {authority}")
            continue
        if authority not in row["install_method"]:
            findings.append(f"developer install method bypasses its authority: {row['id']}: {authority}")
        if authority not in row["removal_method"]:
            findings.append(f"developer removal method bypasses its authority: {row['id']}: {authority}")
    return findings


def validate_receipt(receipt: Path, rows: list[dict[str, str]]) -> list[str]:
    if not receipt.is_file():
        return [f"host receipt missing: {receipt}"]
    findings: list[str] = []
    with receipt.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        required = {"audit_date", "id", "destination", "state", "file_type", "sha256"}
        if not required.issubset(set(reader.fieldnames or ())):
            return ["host receipt header is incomplete"]
        known = {row["id"]: row["destination"] for row in rows}
        seen: set[str] = set()
        for row in reader:
            row_id = row["id"]
            if row_id in seen:
                findings.append(f"duplicate host receipt id: {row_id}")
            seen.add(row_id)
            if row_id not in known:
                findings.append(f"host receipt id missing from manifest: {row_id}")
            elif row["destination"] == "" or (row["destination"] not in known.values() and not row["destination"].startswith("/home/")):
                findings.append(f"host receipt destination is not allowlisted: {row_id}: {row['destination']}")
            if row["state"] == "present":
                if row["file_type"] == "directory":
                    if row["sha256"] != "-":
                        findings.append(f"directory receipt must not fabricate sha256: {row_id}")
                elif not re.fullmatch(r"[0-9a-f]{64}", row["sha256"]):
                    findings.append(f"present receipt has malformed sha256: {row_id}")
    return findings


def run(root: Path, receipt: Path | None = None) -> list[str]:
    manifest = root / "docs/system-install-manifest.tsv"
    rows = load_rows(manifest)
    findings = validate_rows(rows) + validate_wiring(root, rows)
    if receipt is not None:
        findings.extend(validate_receipt(receipt, rows))
    return findings


def self_test() -> None:
    rows = []
    for suffix, destination in (("one", "/one"), ("two", "/two")):
        row = {field: f"{field}-{suffix}" for field in REQUIRED_COLUMNS}
        row["id"] = f"ID-{suffix}"
        row["destination"] = destination
        row["lifecycle"] = "package-managed"
        rows.append(row)
    assert not validate_rows(rows)
    duplicate = [rows[0], rows[0]]
    assert any("duplicate" in finding for finding in validate_rows(duplicate))

    deb_source = """
    install -m755 "$bin" "$work/usr/bin/taskforest-i"
    for size in 16 24; do
        install -Dm644 "$icon_png" "$work/usr/share/icons/hicolor/${size}x${size}/apps/taskforest-taskboard.png"
    done
    install -m644 "$repo/LICENSE" \\
        "$work/usr/share/licenses/taskforest-i/LICENSE"
    cp -a "$target_src/usr" "$work/usr"
    """
    assert deb_install_destinations(deb_source) == {
        "/usr/bin/taskforest-i",
        "/usr/share/icons/hicolor/16x16/apps/taskforest-taskboard.png",
        "/usr/share/icons/hicolor/24x24/apps/taskforest-taskboard.png",
        "/usr/share/licenses/taskforest-i/LICENSE",
    }
    assert rpm_file_destinations(
        "%dir /usr/share/licenses/taskforest-i\n/usr/bin/taskforest-i\n"
    ) == {"/usr/bin/taskforest-i"}

    assert credit_tokens("a or b") == {"a", "b"}
    assert credit_tokens(" a  or  b ") == {"a", "b"}
    assert credit_tokens("packaging/rpm/x.spec or scripts/m install perf") == {
        "packaging/rpm/x.spec",
        "scripts/m install perf",
    }

    # Shared-asset single ownership: only the common provider may install a
    # common destination; a second provider is a regression.
    assert not shared_owner_findings(
        {COMMON_DEB_PROVIDER: {"/a", "/b"}, "packaging/debian/build-deb-x.sh": {"/c"}},
        COMMON_DEB_PROVIDER,
    )
    overlap = shared_owner_findings(
        {COMMON_DEB_PROVIDER: {"/a"}, "packaging/debian/build-deb-x.sh": {"/a"}},
        COMMON_DEB_PROVIDER,
    )
    assert len(overlap) == 1 and "/a" in overlap[0]
    assert shared_owner_findings({}, COMMON_DEB_PROVIDER)

    # Every shared-asset consumer must declare the dependency.
    consumers_controls = {
        name: "Depends: taskforest-common, libc6\n"
        for name in COMMON_CONSUMER_CONTROLS
    }
    consumers_specs = {
        name: "Requires: taskforest-common, glibc\n"
        for name in COMMON_CONSUMER_SPECS
    }
    assert not declared_dependency_findings(consumers_controls, consumers_specs)
    assert declared_dependency_findings(
        {name: "Depends: libc6\n" for name in COMMON_CONSUMER_CONTROLS},
        {name: "Requires: glibc\n" for name in COMMON_CONSUMER_SPECS},
    )
    assert declared_dependency_findings({}, {})

    wix = """<?xml version="1.0" encoding="UTF-8"?>
    <Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
      <Package Name="$(var.ProductName)" Version="1.2.3">
        <StandardDirectory Id="ProgramFiles64Folder">
          <Directory Id="INSTALLFOLDER" Name="TaskForest">
            <Component Id="Product"><File Id="Exe" Source="$(var.StageDir)/$(var.ExeName)" /></Component>
            <Component Id="Notice"><File Id="License" Source="$(var.StageDir)/LICENSE" /></Component>
          </Directory>
        </StandardDirectory>
        <StandardDirectory Id="ProgramMenuFolder">
          <Directory Id="ShortcutFolder" Name="TaskForest">
            <Component Id="Shortcut" Guid="$(var.ShortcutGuid)">
              <Shortcut Id="Entry" Name="$(var.ShortcutName)" />
              <RegistryValue Root="HKLM" Key="Software\\TaskForest" Name="StartMenuShortcut" />
            </Component>
          </Directory>
        </StandardDirectory>
      </Package>
    </Wix>"""
    shared, per_product = wix_destinations(wix)
    assert per_product == {
        "[ProgramFiles64Folder]\\TaskForest\\$(var.ExeName)",
        "[ProgramMenuFolder]\\TaskForest\\$(var.ShortcutName).lnk",
    }
    assert shared == {
        "[ProgramFiles64Folder]\\TaskForest\\LICENSE",
        "HKLM\\Software\\TaskForest\\StartMenuShortcut",
    }
    assert wix_destinations("<Wix xmlns=\"x\"><Package><File Source=\"a/LICENSE\" /></Package></Wix>") == (
        {"\\LICENSE"},
        set(),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--receipt",
        type=Path,
        help="optional local host receipt; do not commit this file to the public repository",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("System-install manifest guard self-test: PASS")
        return 0
    try:
        findings = run(args.repo_root.resolve(), args.receipt)
    except (OSError, ValueError, ET.ParseError) as error:
        print(f"System-install manifest guard: ERROR {error}", file=sys.stderr)
        return 1
    for finding in findings:
        print(f"{finding}")
    print(f"System-install manifest guard: findings={len(findings)}")
    return int(bool(findings))


if __name__ == "__main__":
    raise SystemExit(main())
