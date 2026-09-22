# TaskForest 发布与打包

本文定义公开发布物、触发方式和包验证。安装路径权威见
[SYSTEM_INSTALL_MANIFEST.md](SYSTEM_INSTALL_MANIFEST.md)，质量规则见
[QUALITY_GATES.md](QUALITY_GATES.md)。

## 发布面

当前官方发布矩阵已实现四个独立前端产品（TaskForest-G、TaskForest-I、TaskForest-T、
TaskForest-B）四端发行物与前端语义同权（即「四端平权」）：Linux 同时提供四端的 amd64/arm64 DEB
与 x86_64/aarch64 RPM 发布物，另有共享数据包 `taskforest-common` 的同架构 DEB/RPM；
Windows 同时提供四端的 x64/arm64 MSI 安装包。macOS 打包、签名
和公证暂缓。该平权仅指四端发行物与前端语义同权，不含「跨平台功能对等」；平台能力边界见下方说明。

### 0.1.3 发行面与平价矩阵

正式发行面已扩展至全前端全格式产品体系：Linux 发布流水线原生构建与发布四端（GPUI、
Iced、TUI、Bevy）全量 DEB 与 RPM 安装包；Windows 原生流水线构建与发布四端全量 MSI。
三大图形前端（GPUI、Iced、Bevy）严格保持 Wayland-only，彻底不含 X11 依赖；TUI 为零图形栈
终端渲染，不依赖任何显示服务器（X11 同样不受支持）。
所有发布产物遵循统一命名 `TaskForest-<UI>-<版本>-<平台>.<格式>`（UI 为 `G`/`I`/`T`/`B`）。

该边界只约束发行物，不扩大平台能力。「四端平权」不等于四端「功能对等」，也不表示三个平台
具备相同的系统级能力。发行面内没有合格数据源、授权或原生实现的能力，必须继续以 typed
`Unsupported`、`PermissionRequired`、`RequiresEscalation`、`MissingDependency` 或
`TemporarilyUnavailable` 呈现（这五个是 `platform-contract::CapabilityStatus` 的真实变体名）；
`PermissionDenied` 属失败原因轴 `FailureKind`，与能力级 `PermissionRequired` 是不同轴，不得
混用。平台按 typed 原因诚实降级是产品承诺的一部分，不得用空值、静态占位或未接线按钮把它写成
正式版功能。能力身份承诺面由产品期望面（`CapabilityId::EXPECTED_SURFACE`）声明：runtime
catalog 为每个平台未注册的期望身份发布 typed 缺席 descriptor（`Unsupported`、无 provider
归属），"无条目"不是合法的产品答案；机制与不可逆性见
[ADR-053](../adr/053-product-expected-capability-surface.md)。

只有推送与根 `Cargo.toml` 版本一致的 `vX.Y.Z` tag，才会创建正式 Release 并生成以下 28 项
产物：四端各 4 个 DEB/RPM 包，加共享数据包 `taskforest-common` 的两架构 DEB/RPM。
所有发布产物遵循统一命名 `TaskForest-<UI>-<版本>-<平台>.<格式>`（UI 对应为 `G`、`I`、`T`、`B`，
共享数据包用 `Common`，平台为 `x64`/`arm64`）；权威定义见 [PRODUCT_IDENTITY.md](PRODUCT_IDENTITY.md)。
包内元数据仍遵守发行版惯例：DEB `Architecture` 为 `amd64`/`arm64`，RPM arch 为
`x86_64`/`aarch64`，与文件名中的 `x64`/`arm64` 是固定映射。

| 平台 | 架构 | 产物 | 包内 arch | 构建入口 |
|---|---|---|---|---|
| Linux | x64 | `TaskForest-G-<ver>-x64.deb` | `amd64` | `packaging/debian/build-deb.sh` |
| Linux | arm64 | `TaskForest-G-<ver>-arm64.deb` | `arm64` | `packaging/debian/build-deb.sh` |
| Linux | x64 | `TaskForest-I-<ver>-x64.deb` | `amd64` | `packaging/debian/build-deb-iced.sh` |
| Linux | arm64 | `TaskForest-I-<ver>-arm64.deb` | `arm64` | `packaging/debian/build-deb-iced.sh` |
| Linux | x64 | `TaskForest-T-<ver>-x64.deb` | `amd64` | `packaging/debian/build-deb-tui.sh` |
| Linux | arm64 | `TaskForest-T-<ver>-arm64.deb` | `arm64` | `packaging/debian/build-deb-tui.sh` |
| Linux | x64 | `TaskForest-B-<ver>-x64.deb` | `amd64` | `packaging/debian/build-deb-bevy.sh` |
| Linux | arm64 | `TaskForest-B-<ver>-arm64.deb` | `arm64` | `packaging/debian/build-deb-bevy.sh` |
| Linux | x64 | `TaskForest-G-<ver>-x64.rpm` | `x86_64` | `packaging/rpm/build-rpm.sh ... G` |
| Linux | arm64 | `TaskForest-G-<ver>-arm64.rpm` | `aarch64` | `packaging/rpm/build-rpm.sh ... G` |
| Linux | x64 | `TaskForest-I-<ver>-x64.rpm` | `x86_64` | `packaging/rpm/build-rpm.sh ... I` |
| Linux | arm64 | `TaskForest-I-<ver>-arm64.rpm` | `aarch64` | `packaging/rpm/build-rpm.sh ... I` |
| Linux | x64 | `TaskForest-T-<ver>-x64.rpm` | `x86_64` | `packaging/rpm/build-rpm.sh ... T` |
| Linux | arm64 | `TaskForest-T-<ver>-arm64.rpm` | `aarch64` | `packaging/rpm/build-rpm.sh ... T` |
| Linux | x64 | `TaskForest-B-<ver>-x64.rpm` | `x86_64` | `packaging/rpm/build-rpm.sh ... B` |
| Linux | arm64 | `TaskForest-B-<ver>-arm64.rpm` | `aarch64` | `packaging/rpm/build-rpm.sh ... B` |
| Linux | x64 | `TaskForest-Common-<ver>-x64.deb` | `amd64` | `packaging/debian/build-deb-common.sh` |
| Linux | arm64 | `TaskForest-Common-<ver>-arm64.deb` | `arm64` | `packaging/debian/build-deb-common.sh` |
| Linux | x64 | `TaskForest-Common-<ver>-x64.rpm` | `x86_64` | `packaging/rpm/build-rpm.sh ... C` |
| Linux | arm64 | `TaskForest-Common-<ver>-arm64.rpm` | `aarch64` | `packaging/rpm/build-rpm.sh ... C` |
| Windows | x64 | `TaskForest-G-<ver>-x64.msi` | `x64` | `packaging/windows/build-msi.sh ... G` |
| Windows | arm64 | `TaskForest-G-<ver>-arm64.msi` | `arm64` | `packaging/windows/build-msi.sh ... G` |
| Windows | x64 | `TaskForest-I-<ver>-x64.msi` | `x64` | `packaging/windows/build-msi.sh ... I` |
| Windows | arm64 | `TaskForest-I-<ver>-arm64.msi` | `arm64` | `packaging/windows/build-msi.sh ... I` |
| Windows | x64 | `TaskForest-T-<ver>-x64.msi` | `x64` | `packaging/windows/build-msi.sh ... T` |
| Windows | arm64 | `TaskForest-T-<ver>-arm64.msi` | `arm64` | `packaging/windows/build-msi.sh ... T` |
| Windows | x64 | `TaskForest-B-<ver>-x64.msi` | `x64` | `packaging/windows/build-msi.sh ... B` |
| Windows | arm64 | `TaskForest-B-<ver>-arm64.msi` | `arm64` | `packaging/windows/build-msi.sh ... B` |

任一 DEB、RPM 或 MSI 架构缺失或验证失败，发布必须失败。Windows 不再是可选或
`continue-on-error` 平台。

Linux amd64/arm64 和 Windows x64/arm64 均使用对应的 GitHub-hosted 原生 runner，
构建 target 与包架构由同一矩阵项绑定，不能用 x64 二进制伪装 arm64 包。

## Linux 安装树

`packaging/arch/PKGBUILD::package()` 是系统安装树的唯一布局权威：

1. `packaging/linux/stage-release-tree.sh` 从该布局生成 staged tree；
2. DEB 和 RPM 构建器只叠加格式元数据；共享图标资产由数据包 `taskforest-common`
   独占，四端包从自身文件清单移除这些路径并声明对该包的依赖；
3. `packaging/arch/stage-package-sim.sh` 检查 manifest、权限和 polkit `exec.path`；
4. 发布包只包含 DEB/RPM 的系统安装树；正式发布面不包含便携包或后台服务。

## Windows MSI

WiX 文件 `packaging/windows/taskforest.wxs` 是 MSI 文件清单权威。该文件对四端参数化，
每个 `TaskForest-<UI>` MSI 安装所选前端的可执行文件（`G`/`I`/`T`/`B`）、同目录的身份校验
UAC process-control helper、LICENSE、THIRD-PARTY-NOTICES 和开始菜单入口，不安装后台历史
服务或 autostart。Linux 专用 helper 和 polkit policy 不进入 MSI。共享的 helper、LICENSE、
notices 与安装标记使用四端一致的组件 GUID，由 Windows Installer 引用计数，只在最后一个
产品卸载时移除；不设单独的 common MSI。

MSI 的 `ProductVersion` 属性受 Windows Installer 硬性限制只能为数字段 `X.Y.Z`；完整版本
（含 `rcN`）出现在文件名、MSI 摘要 Description、ARP comments 以及安装后的
`Software\TaskForest\Version` 注册表值中，CI 会在反编译校验里断言完整版本确实入包。
rc 与正式版共享数字 `ProductVersion`，覆盖升级由 `AllowSameVersionUpgrades` 保证。

CI 在构建后使用 Windows Installer 管理提取验证 MSI 数据库和关键文件。配置
`WINDOWS_CERT_B64` 与 `WINDOWS_CERT_PASSWORD` 时执行 Authenticode 签名；没有证书时
可以生成明确标注的未签名预发布包，但 SmartScreen 可能警告。

## 版本与 tag

- tag 形如 `vX.Y.Z` 或 `vX.Y.Z-rcN`；预发布后缀统一连写不带点
  （如 `v0.1.0-rc5`），与 Cargo 版本、产物文件名逐字一致；
  semver 按字典序比较预发布标识（`rc10` 会排在 `rc5` 之前），因此 `rcN`
  只用于个位数编号，需要更多轮次时应直接发布正式版；
- 默认要求 tag 版本与根 `Cargo.toml` 完全一致；
- `Cargo.lock` 必须提交且 `cargo metadata --locked` 通过；
- prerelease tag 自动创建 GitHub prerelease；
- DEB/RPM 的版本字段禁止预发布连字符：`0.1.0-rc5` 落盘为 `0.1.0~rc5`
  （~ 排序低于正式版，保证 rc 可被 `0.1.0` 升级覆盖）；
- MSI 文件名使用完整 Cargo 版本（含 `rcN` 预发布后缀），与其他产物一致；
  MSI `ProductVersion` 属性只使用数字段 `X.Y.Z`（WiX 要求），由 CI 从完整版本剥离；
- 每个平台输出独立 SHA-256 清单；
- tag 一经推送不可移动、重打或删除重发；
- prerelease 编号连续递增，作废编号不回收，缺失原因在根
  [CHANGELOG.md](../CHANGELOG.md) 中说明。

## 分支与发布线生命周期

- `main` 是唯一开发主线；发布稳定期的收尾工作在 `release/X.Y` 分支进行，经 PR 并回
  `main` 后由 `main` 打 tag 发布；
- 发布分支在特性集冻结时创建，只接受修复性 cherry-pick；对应最终 tag 发布后即删除；
- 已合并的历史分支（RC 分支、修复分支）及时删除，远端只保留 `main` 与活跃发布线；
- 已推送的分支与 tag 禁止 force-push、重打或移动。

## 预发布（RC）准出

RC 仅在特性集冻结后切出；日常验证使用 `main` 的 CI 构建与手动 Rehearsal，不消耗
RC 编号。打 RC tag 前必须满足：

1. 对应提交上 Linux 与 Windows 阻塞 CI 为绿，Rehearsal 的 Linux/Windows x64 与
   arm64 job 全部成功；
2. 版本号已在根 `Cargo.toml` 提升，并与 tag 同一次推送发布；
3. 上一 RC 遗留问题在 Release notes 中声明状态；
4. 打包行为变化已在本文或 Release notes 中说明。

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
4. 删除已合并的 RC 分支与修复分支，远端只保留 `main` 与活跃发布线；
5. 推送首个版本 tag，例如 `v0.1.0`；
6. 核对 GitHub Release 同时包含全部必需架构产物和 SHA-256 清单，并在
   [CHANGELOG.md](../CHANGELOG.md) 中定稿 `0.1.0` 条目。
