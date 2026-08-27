# TaskForest 发布与打包

本文定义公开发布物、触发方式和包验证。安装路径权威见
[SYSTEM_INSTALL_MANIFEST.md](SYSTEM_INSTALL_MANIFEST.md)，质量规则见
[QUALITY_GATES.md](QUALITY_GATES.md)。

## 发布面

当前发行包只包含 GPUI / TaskForestG。Iced、TUI 和 Bevy 不进入二进制发行包；macOS
打包、签名和公证暂缓。

只有推送与根 `Cargo.toml` 版本一致的 `vX.Y.Z` tag，才会创建正式 Release 并生成以下产物：

| 平台 | 架构 | 产物 | 构建入口 |
|---|---|---|---|
| Linux | amd64 | `taskforest_<ver>_amd64.deb` | `packaging/debian/build-deb.sh` |
| Linux | arm64 | `taskforest_<ver>_arm64.deb` | `packaging/debian/build-deb.sh` |
| Linux | x86_64 | `taskforest-<ver>-1.x86_64.rpm` | `packaging/rpm/build-rpm.sh` |
| Linux | aarch64 | `taskforest-<ver>-1.aarch64.rpm` | `packaging/rpm/build-rpm.sh` |
| Windows | x64 | `TaskForest-<ver>-x64.msi` | `packaging/windows/build-msi.sh`（WiX） |
| Windows | arm64 | `TaskForest-<ver>-arm64.msi` | `packaging/windows/build-msi.sh`（WiX） |

任一 DEB、RPM 或 MSI 架构缺失或验证失败，发布必须失败。Windows 不再是可选或
`continue-on-error` 平台。

Linux amd64/arm64 和 Windows x64/arm64 均使用对应的 GitHub-hosted 原生 runner，
构建 target 与包架构由同一矩阵项绑定，不能用 x64 二进制伪装 arm64 包。

## Linux 安装树

`packaging/arch/PKGBUILD::package()` 是系统安装树的唯一布局权威：

1. `packaging/linux/stage-release-tree.sh` 从该布局生成 staged tree；
2. DEB 和 RPM 构建器只叠加格式元数据；
3. `packaging/arch/stage-package-sim.sh` 检查 manifest、权限和 polkit `exec.path`；
4. 发布包只包含 DEB/RPM 的系统安装树；正式发布面不包含便携包或后台服务。

## Windows MSI

WiX 文件 `packaging/windows/taskforest.wxs` 是 MSI 文件清单权威。MSI 安装 GPUI 可执行文件、
LICENSE 和开始菜单入口，不安装后台历史服务或 autostart。

CI 在构建后使用 Windows Installer 管理提取验证 MSI 数据库和关键文件。配置
`WINDOWS_CERT_B64` 与 `WINDOWS_CERT_PASSWORD` 时执行 Authenticode 签名；没有证书时
可以生成明确标注的未签名预发布包，但 SmartScreen 可能警告。

## 版本与 tag

- tag 形如 `vX.Y.Z` 或 `vX.Y.Z-pre`；
- 默认要求 tag 版本与根 `Cargo.toml` 完全一致；
- `Cargo.lock` 必须提交且 `cargo metadata --locked` 通过；
- prerelease tag 自动创建 GitHub prerelease；
- DEB/RPM 的版本字段禁止预发布连字符：`0.1.0-rc.1` 落盘为 `0.1.0~rc.1`
  （~ 排序低于正式版，保证 rc 可被 `0.1.0` 升级覆盖）；
- MSI ProductVersion 只使用数字段 `X.Y.Z`；
- 每个平台输出独立 SHA-256 清单。

## Workflow

| Workflow | 触发 | 作用 |
|---|---|---|
| `ci.yml` | PR、main push、手动 | 合并质量门，不生产发布物 |
| `portability.yml` | PR、main push、手动、月度 | Windows（PR/main 阻塞）与 macOS（手动/月度 advisory）原生边界检查 |
| `packaging.yml` | 仅 `workflow_call` | 构建和验证 Linux/Windows 多架构产物 |
| `rehearsal.yml` | 手动 | 同时排练 Linux 与 Windows，不发布 |
| `release.yml` | `v*` tag、指定已有 tag 手动 | history/precheck → packaging → 单一发布 |

Release 只把 `contents: write` 授予发布 job。可复用 workflow 只接收两个显式 Windows
签名 secret，不继承其他仓库 secret。

## 发布事务

1. precheck 验证 tag、版本、锁文件、安装清单和公开历史边界；
2. Linux amd64/arm64 与 Windows x64/arm64 并行构建并上传 workflow artifact；
3. Linux 验证双架构 DEB/RPM，Windows 验证双架构 MSI；
4. publish job 下载所有 artifact，断言全部架构和校验文件齐全；
5. 所有断言成功后一次性附加到 GitHub Release。

该顺序避免 Linux 已发布而 Windows 失败的半成品 Release。

## 首次公开发布前

1. 完成 Git 历史脱敏并通过 `public_repo_guard.py --history`；
2. 手动运行 Rehearsal，确认 Linux/Windows 的 x64 与 arm64 job 均为绿色；
3. 在干净 Linux 环境检查 DEB/RPM，在 Windows 环境执行 MSI 安装、升级和卸载；
4. 推送首个版本 tag，例如 `v0.1.0`；
5. 核对 GitHub Release 同时包含全部必需架构产物和 SHA-256 清单。
