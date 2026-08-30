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

Field offsets and sentinels follow dmidecode's `dmi_decode` cases 0/1/2/17
(size word with KB-unit bit, form-factor and memory-type enums, string-set
decoding with the double-NUL terminator, SMBIOS 2.6 configured-speed and UUID
length gates, the mixed-endian UUID rendering of `dmi_system_uuid`). Verified
by headless byte-fixture tests:

```bash
cargo nextest run -p taskmanager-smbios-tables -j 4 --all-targets
cargo clippy -p taskmanager-smbios-tables --all-targets -- -D warnings
```
