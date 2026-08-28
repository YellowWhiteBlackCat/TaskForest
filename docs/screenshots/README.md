# Public screenshot policy

This directory intentionally contains no host captures.

Public screenshots may be committed only when they are generated from deterministic demo data and
have been reviewed for:

- usernames, email addresses, hostnames and user paths;
- SSIDs, IP/MAC addresses, socket paths and remote endpoints;
- process/application lists and window titles from the capture host;
- device serials, volume labels, boot identifiers and account-specific configuration;
- embedded EXIF or other image metadata.

Capture scripts may write local evidence here during development, but `.gitignore` excludes every
file in this directory except this policy. Accepted public product images should use a separate,
explicitly reviewed asset path in the future.

## Backend qualification

Nested Niri is the semantic compositor route for window ownership, output geometry and
layer-shell behavior. If its IPC socket is created but requests time out after a client maps,
record the run as `BLOCKED (compositor/backend)` and keep the local evidence; do not replace
accepted images with an older run.

Gamescope is an auxiliary route for a single app's real pixels and fixed-size responsive-layout
review. Its advertised layer-shell global is not sufficient evidence that layer surfaces are
composited or that desktop semantics match Niri. Until a dedicated, independently validated
layer-shell receipt exists, gamescope captures are not Layer-Shell acceptance evidence.
