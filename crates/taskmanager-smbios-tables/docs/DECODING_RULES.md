# Decoding rules

Field offsets and sentinels follow dmidecode's `dmi_decode` cases 0/1/2/17:
the size word with its KB-unit bit, form-factor and memory-type enums,
string-set decoding with the double-NUL terminator, SMBIOS 2.6
configured-speed and UUID length gates, and the mixed-endian UUID rendering
of `dmi_system_uuid`. The honesty rules (no I/O, no dependencies, no unsafe,
`None` over fabrication, total parsing) are the crate boundary in the README.

## Verification

Verified by headless byte-fixture tests:

```bash
cargo nextest run -p taskmanager-smbios-tables -j 4 --all-targets
cargo clippy -p taskmanager-smbios-tables --all-targets -- -D warnings
```
