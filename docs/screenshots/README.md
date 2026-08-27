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
