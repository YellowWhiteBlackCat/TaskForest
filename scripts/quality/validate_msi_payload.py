#!/usr/bin/env python3
"""Validate decompiled MSI metadata and its actual extracted file references."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path, PureWindowsPath
import struct
import tempfile
import xml.etree.ElementTree as ET


def tag(element: ET.Element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def pe_machine(path: Path) -> int:
    with path.open("rb") as stream:
        header = stream.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise ValueError(f"invalid DOS header: {path.name}")
        offset = struct.unpack_from("<I", header, 60)[0]
        if offset < 64 or offset > path.stat().st_size - 6:
            raise ValueError(f"invalid PE offset: {path.name}")
        stream.seek(offset)
        signature = stream.read(6)
        if signature[:4] != b"PE\0\0":
            raise ValueError(f"invalid PE signature: {path.name}")
        return struct.unpack_from("<H", signature, 4)[0]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate(xml: Path, stage: Path, ui: str, arch: str, version: str, full: str) -> None:
    root = ET.parse(xml).getroot()
    packages = [element for element in root.iter() if tag(element) == "Package"]
    if len(packages) != 1:
        raise ValueError("decompiled MSI must contain exactly one Package")
    package = packages[0]
    if package.get("Name") != f"TaskForest-{ui}" or package.get("Version") != version:
        raise ValueError("MSI product identity or numeric version differs")
    if not any(tag(element) == "Property" and element.get("Id") == "ARPCOMMENTS"
               and full in element.get("Value", "") for element in root.iter()):
        raise ValueError("full version missing from MSI ARP metadata")
    if not any(tag(element) == "RegistryValue" and element.get("Key") == r"Software\TaskForest"
               and element.get("Name") == "Version" and element.get("Value") == full
               for element in root.iter()):
        raise ValueError("installed full-version registry entry differs")
    expected = {
        "TaskForestExe": f"taskforest-{ui.lower()}.exe",
        "TaskForestProcessControlHelperExe": "taskmanager-process-control-helper.exe",
        "LicenseFile": "LICENSE",
        "ThirdPartyNoticesFile": "THIRD-PARTY-NOTICES.txt",
    }
    files = [element for element in root.iter() if tag(element) == "File"]
    if len(files) != len(expected) or {element.get("Id") for element in files} != set(expected):
        raise ValueError("MSI file table contains missing, duplicate or unexpected payloads")
    extraction = xml.parent.resolve()
    for element in files:
        name = expected[element.get("Id", "")]
        if element.get("Name", "").split("|")[-1] != name:
            raise ValueError(f"installed payload name differs: {name}")
        source = element.get("Source")
        if not source:
            raise ValueError(f"payload reference missing: {name}")
        # WiX 7 authoring keeps a virtual SourceDir prefix even when -x
        # extracts the corresponding File/<id> under the explicit export root.
        reference = PureWindowsPath(source)
        path = extraction.joinpath(*reference.parts[1:]) if reference.parts[:1] == ("SourceDir",) else Path(source)
        if not path.is_absolute():
            path = xml.parent / path
        path = path.resolve()
        if not path.is_relative_to(extraction) or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"extracted payload is absent or outside its owned directory: {name} ({source!r})")
        original = stage / name
        if not original.is_file() or digest(path) != digest(original):
            raise ValueError(f"MSI cabinet payload differs from the staged input: {name}")
        if name.endswith(".exe") and pe_machine(path) != {"x64": 0x8664, "arm64": 0xAA64}[arch]:
            raise ValueError(f"native PE architecture differs: {name}")


def self_test() -> None:
    scratch = Path(__file__).resolve().parents[2] / ".tmp"
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="msi-payload-", dir=scratch) as temporary:
        base = Path(temporary)
        stage = base / "stage"
        extraction = base / "extract"
        stage.mkdir()
        extraction.mkdir()
        package = ET.Element("Package", Name="TaskForest-G", Version="0.2.0")
        ET.SubElement(package, "Property", Id="ARPCOMMENTS", Value="TaskForest-G 0.2.0")
        ET.SubElement(package, "RegistryValue", Key=r"Software\TaskForest", Name="Version", Value="0.2.0")
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 64)
        pe[64:68] = b"PE\0\0"
        struct.pack_into("<H", pe, 68, 0x8664)
        for identity, name in [("TaskForestExe", "taskforest-g.exe"),
                               ("TaskForestProcessControlHelperExe", "taskmanager-process-control-helper.exe"),
                               ("LicenseFile", "LICENSE"), ("ThirdPartyNoticesFile", "THIRD-PARTY-NOTICES.txt")]:
            payload = bytes(pe) if name.endswith(".exe") else b"license terms"
            (stage / name).write_bytes(payload)
            (extraction / name).write_bytes(payload)
            ET.SubElement(package, "File", Id=identity, Name=name, Source=name)
        xml = extraction / "package.wxs"
        # Real WiX decompilation uses File IDs, not installed filenames.
        (extraction / "File").mkdir()
        for element in package.iter("File"):
            identity = element.get("Id")
            (extraction / "File" / identity).write_bytes((stage / element.get("Name")).read_bytes())
            element.set("Source", "SourceDir\\File\\" + identity)
        ET.ElementTree(package).write(xml)
        validate(xml, stage, "G", "x64", "0.2.0", "0.2.0")
        mutations = [lambda: package.set("Version", "0.1.0"),
                     lambda: package.find("File").set("Name", "wrong.exe"),
                     lambda: package.find("File").set("Source", "../stage/taskforest-g.exe"),
                     lambda: (extraction / "File" / "TaskForestExe").write_bytes(b"broken"),
                     lambda: package.remove(package.find("File"))]
        for mutate in mutations:
            original = ET.tostring(package)
            mutate()
            ET.ElementTree(package).write(xml)
            try:
                validate(xml, stage, "G", "x64", "0.2.0", "0.2.0")
            except ValueError:
                pass
            else:
                raise ValueError("corrupt metadata or payload was accepted")
            package = ET.fromstring(original)
            (extraction / "File" / "TaskForestExe").write_bytes(bytes(pe))
        ET.ElementTree(package).write(xml)
        try:
            validate(xml, stage, "G", "arm64", "0.2.0", "0.2.0")
        except ValueError:
            pass
        else:
            raise ValueError("wrong native architecture was accepted")
    print("MSI metadata/payload validator self-test: PASS")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--xml", type=Path)
    parser.add_argument("--stage", type=Path)
    parser.add_argument("--frontend", choices=list("GITB"))
    parser.add_argument("--arch", choices=["x64", "arm64"])
    parser.add_argument("--version")
    parser.add_argument("--full-version")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if any(value is None for value in [args.xml, args.stage, args.frontend, args.arch, args.version, args.full_version]):
        parser.error("all MSI validation arguments are required")
    validate(args.xml, args.stage, args.frontend, args.arch, args.version, args.full_version)
    print(f"MSI database and cabinet payloads: PASS ({args.frontend}, {args.arch})")


if __name__ == "__main__":
    main()
