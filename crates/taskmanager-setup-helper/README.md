# taskmanager-setup-helper

## Role

Fixed Linux first-run setup helper for the optional RAPL/udev integration.

## Boundary

Only the explicit install/revert vocabulary is accepted. The helper validates
the target, writes through bounded atomic steps, reloads the relevant subsystem
and rolls back on failure; it is not a general installer or shell.

## Contract and verification

Conflict, permission, reload failure, retry, revert and restart outcomes are
typed. Test the helper in isolation and keep installed-package polkit and
restart receipts separate from fixture evidence.
