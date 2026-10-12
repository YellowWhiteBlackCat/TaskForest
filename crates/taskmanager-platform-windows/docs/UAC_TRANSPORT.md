# UAC transport (ADR-035)

`provider::process::uac` pre-creates the one-shot randomly named reply file,
builds the fixed helper command line with the escalation crate's pure
builder, and drives the audited `runas` call group; every raw result is
classified by `taskmanager-escalation::uac`'s typed transport facts.

A normal inherited-token child is never treated as elevation. Shipping,
compile verification and the receipt-before-release-claim rule are transport
facts owned by
[`taskmanager-escalation`](../../taskmanager-escalation/docs/PLATFORM_TRANSPORTS.md).
