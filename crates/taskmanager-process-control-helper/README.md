# taskmanager-process-control-helper

## Role

One-shot privileged Linux helper for exactly one identity-checked process
control operation per polkit invocation.

## Boundary

It receives a fixed operation and frozen PID/start identity, revalidates before
the syscall and returns a typed result. It does not expose a shell, batch
control, blanket capability or UI state.

## Contract and verification

Success, target exit, PID reuse, permission, invalid operation and timeout must
remain distinguishable. Package staging, policy checks and live installed
authorization are independent evidence requirements.
