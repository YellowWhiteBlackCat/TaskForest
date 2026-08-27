# taskmanager-privilege-helper

## Role

One-shot Linux helper for Intel GPU PMU reads, invoked through the escalation
seam and emitting a typed JSON envelope.

## Boundary

The helper accepts only the fixed `--gpu-engines` operation, owns its privileged
resource for that invocation and never becomes a general command runner.

## Contract and verification

Exit code, stderr, JSON schema, denial and malformed input are part of the
contract. Package/polkit verification and a live perf receipt are separate;
without the latter the capability remains permission-limited.
