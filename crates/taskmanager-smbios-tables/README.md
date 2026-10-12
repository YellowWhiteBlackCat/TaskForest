# taskmanager-smbios-tables

## Role

The one authority for SMBIOS record parsing: a pure, safe, zero-dependency
decoder for Memory Device (type 17), BIOS Information (type 0), System
Information (type 1), and Base Board Information (type 2) structures, shared
by the unprivileged Linux adapter and the pkexec-escalated SMBIOS helper so
both decode the same bytes through the same rules.

## Boundary

No I/O, no dependencies, no unsafe. Input is the raw structure bytes exactly
as exported under `/sys/firmware/dmi/entries/<type>-N/raw`. Absent, unknown,
or version-missing facts are `None` — never a fabricated zero or string. The
parser is total: a truncated record yields the fields that fit; `None` at an
entry point means the record is not that structure type at all.

## Contract and verification

The dmidecode-compatible offset/sentinel rules and the byte-fixture
verification route are specified in
[`docs/DECODING_RULES.md`](docs/DECODING_RULES.md).

## Module map

```text
src/lib.rs   the ONE pure SMBIOS record parser, shared by the unprivileged
             DMI probe and the pkexec-escalated helper (types 0/1/2/17)
```

## Docs

- [`docs/DECODING_RULES.md`](docs/DECODING_RULES.md) — dmidecode-compatible offsets and
  sentinels, honesty rules and verification.
