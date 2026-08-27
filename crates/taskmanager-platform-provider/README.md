# taskmanager-platform-provider

## Role

Platform-neutral provider SPI, capability descriptors, registration and typed
provider identity.

## Boundary

The SPI describes observation/control behavior but does not choose an OS,
perform I/O, own worker threads or render a result.

## Contract and verification

Each registered capability has one provider, one route and one bounded lane.
Keep request/outcome types exhaustive and verify catalog completeness,
registration bijection and platform adapter conformance.
