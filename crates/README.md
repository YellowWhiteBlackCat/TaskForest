# Crate 细节索引

这是第三层的入口。每个 crate README 只描述该 crate 的职责、拥有的事实、禁止拥有的边界、
公共合同和验证方式；跨 crate 当前规则仍以 `docs/` 总纲为准。

## Shared core

- [core](taskmanager-core/README.md) · [application](taskmanager-application/README.md) · [shell](taskmanager-shell/README.md)
- [UI contract](taskmanager-ui-contract/README.md) · [platform contract](taskmanager-platform-contract/README.md)
- [provider SPI](taskmanager-platform-provider/README.md) · [portable providers](taskmanager-platform-portable/README.md) · [runtime](taskmanager-platform-runtime/README.md)
- [telemetry store](taskmanager-telemetry-store/README.md) · [history store](taskmanager-history-store/README.md)

## Platform and composition

- [app host](taskmanager-app-host/README.md) · [native selection](taskmanager-platform-native/README.md)
- [Linux](taskmanager-platform-linux/README.md) · [macOS](taskmanager-platform-macos/README.md) · [Windows](taskmanager-platform-windows/README.md) · [Android (feature-gated)](taskmanager-platform-android/README.md) · [OpenHarmony](taskmanager-platform-ohos/README.md)
- [conformance](taskmanager-platform-conformance/README.md) · [accessibility](taskmanager-accessibility-linux/README.md)

## Product surfaces

- [GPUI](taskmanager-gpui/README.md) · [Iced](taskmanager-iced/README.md) · [TUI](taskmanager-tui/README.md) · [Bevy](taskmanager-bevy-ui/README.md)
- [UI components](taskmanager-ui/README.md) · [theme](taskmanager-theme/README.md)
- [icons](taskmanager-icons/README.md) · [assets](taskmanager-assets/README.md) · [tray](taskmanager-tray-muda/README.md)

## Audited boundaries and helpers

- [perf ioctl](taskmanager-perf-ioctl/README.md) · [AF_PACKET](taskmanager-afpacket/README.md)
- [fd bridge](taskmanager-fd-bridge/README.md) · [Windows API](taskmanager-windows-api/README.md)
- [escalation](taskmanager-escalation/README.md) · [net launcher](taskmanager-net-launcher/README.md)
- [privilege helper](taskmanager-privilege-helper/README.md) · [process-control helper](taskmanager-process-control-helper/README.md)
- [setup helper](taskmanager-setup-helper/README.md)
- [smbios tables](taskmanager-smbios-tables/README.md) — the ONE pure parser for SMBIOS records, shared by the unprivileged DMI probe and the helper
- [smbios helper](taskmanager-smbios-helper/README.md) · [rapl helper](taskmanager-rapl-helper/README.md) · [msr helper](taskmanager-msr-helper/README.md)

## Test support

- [test support](taskmanager-test-support/README.md) — dev-only typed fixture
  builders for behavior tests; consumed only through dev-dependencies, never a
  product dependency.

The fuzz workspaces have their own manifests and remain test-only; they do not define product
architecture or a runtime capability.
