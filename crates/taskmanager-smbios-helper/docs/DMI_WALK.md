# DMI walk

The helper performs exactly one operation: walk
`/sys/firmware/dmi/entries/17-*/raw` (Memory Device records) plus the first
`0-*`/`1-*`/`2-*` entries (BIOS / System / Base Board identity) through the
shared SMBIOS format authority (`taskmanager-smbios-tables`) and emit slot
counts, the populated modules, and the identity object (`null` when the host
carries no type-0/1/2 entries).

## Verification

Exit codes (0 success; 2 permission_denied, 3 no_dmi, 4 open_failed, 5
read_failed), the field-disjoint SUCCESS/ERROR envelopes,
null-for-unstated module and identity fields, slot counting (populated,
empty, malformed), slot sorting, the identity walk (first entry per type,
honest null when the tables are absent), and denial classification are
covered by headless fixture tests:

```bash
cargo nextest run -p taskmanager-smbios-helper -j 4 --all-targets
cargo clippy -p taskmanager-smbios-helper --all-targets -- -D warnings
```

Package/polkit verification and a live DMI receipt are separate; without the
latter the capability remains permission-limited.
