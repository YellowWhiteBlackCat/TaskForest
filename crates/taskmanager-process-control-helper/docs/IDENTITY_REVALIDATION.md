# Identity revalidation

The helper receives a fixed operation and frozen PID/start identity,
revalidates before the syscall and returns a typed result. It does not expose
a shell, batch control, blanket capability or UI state.

Success, target exit, PID reuse, permission, invalid operation and timeout
must remain distinguishable outcomes.

On Windows the helper speaks the UAC one-shot reply-file protocol (ADR-035);
the typed transport vocabulary lives in `taskmanager-escalation::uac` and the
audited call group in `taskmanager-windows-api`.

Linux package staging/policy checks and Windows MSI payload checks are
separate from live installed authorization evidence.

```bash
cargo nextest run -p taskmanager-process-control-helper -j 4 --all-targets
```
