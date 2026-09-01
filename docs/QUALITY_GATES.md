# TaskForest 质量门禁

本文定义当前公开仓库的验证层级。门禁证明行为、边界和发布构造，不使用内部评分、截图数量
或主机回执替代产品验证。

## 1. 合并门

对 `main` 的 pull request 和 push 同时运行 Linux 与 Windows 阻塞 CI：

| 检查 | 目的 |
|---|---|
| public-repo guard | 拒绝私有路径、真实截图、个人邮箱、凭据模式和主机路径 |
| cargo-deny | 拒绝已知漏洞、不允许的许可证和依赖策略违规 |
| Python/Shell 政策门 | 检查安装清单、自动化安全、模块边界和测试布局 |
| rustfmt / clippy | 格式和 warning-free 编译 |
| release build | 验证默认 GPUI 发布形态（PR 用无 LTO 冒烟 profile，main 与 tag 用完整发布 profile） |
| package stage simulation | 验证 Linux 安装树、权限和 polkit 路径 |
| workspace nextest | 行为和平台无关契约 |
| doctests / rustdoc | 文档代码和公开 API 链接 |
| fallback feature matrix | 验证可选 provider 不产生产品 SKU 分叉 |
| CORE-04 functional matrix | 验证每个产品意图在 GPUI/Iced/TUI/Bevy 都有显式 surface decision |

CI 与本地门禁都使用 [`rust-toolchain.toml`](../rust-toolchain.toml) 声明的 stable 最新版；
`Cargo.toml` 的 `rust-version` 仅是兼容性下限。所有 Cargo 验证使用锁文件，并行度不超过四。
Windows 原生边界以及 macOS 编译/库测试在每次 PR 与 main push 的 portability workflow 中阻塞运行；
macOS 打包和真实设备视觉验证仍然 deferred，跨平台编译不能替代原生 API 或设备证据。

## 2. 本地层级

统一入口为：

```bash
bash scripts/quality/local-gates.sh quick
bash scripts/quality/local-gates.sh standard
bash scripts/quality/local-gates.sh extended
```

- `quick`：公开边界、文档、格式、依赖版本底线、模块、安装清单、自动化、测试执行器和测试布局政策门；其中 `scripts/quality/test_runner_guard.py` 机械拒绝非 doctest 的裸 Cargo 测试入口及缺少四并行度的测试执行。具备宿主 Wayland/KWin 依赖时还运行真实私有 A/B 隔离测试；可用 `TM_CAPTURE_ISOLATION_GATE=1` 强制运行，缺少环境时 `auto` 只记录明确的 SKIP。
- `standard`：quick + dependency audit、clippy、nextest、doctest、rustdoc、release build 和
  平台无关形态矩阵，以及 Linux release/package smoke；
- `extended`：standard + coverage、mutation、Miri、fuzz 和性能/体积回归。

**范围隔离**：并行前端线共用一个工作区时，追加 `--scope <core|bevy|gpui|iced|tui>`
把 cargo 阶段（fmt/clippy/nextest/doctests/rustdoc）和源码面政策门限制在该前端的
"core + 依赖闭包 + 自身 crate"（闭包由 `cargo tree` 从锁文件推导，不是手工清单）；
跨前端的根验收层、形态矩阵和 release smoke 记为 SKIP，由主线/合并负责人在 `all`
（默认）下承担。例：Bevy 线的日常门禁是
`bash scripts/quality/local-gates.sh standard --scope bevy`（Bevy 交互矩阵为 headless，
scoped 下随 standard 直接运行）。

**共享锁降级**：开发阶段若另一条线正持续改写共享 Cargo.lock，`lock-consistency`
探针等不到稳定窗口时，本次运行显式降级为不带 `--locked`（summary 记 FALLBACK，
子脚本经 `TM_CARGO_LOCK` 同步），而不是各阶段以晦涩的锁错误失败；合并前合并
负责人仍须以 `all` + 锁定模式重跑。

默认遇到首个失败即退出；需要一次性收集多个失败时显式追加 `--keep-going`。Linux
`release/package smoke` 与 CI 使用同一入口，平台不具备 Linux 打包能力时不在 Windows/macOS
本地门禁中伪造通过。

Windows 开发机使用 `scripts/windows/local-gates.sh`，通过 Git Bash 调用同一组可移植门禁；
Windows telemetry、测试和 helper 不使用 PowerShell 或其他命令解释器采集系统事实。

## 3. 公开仓库门禁

`scripts/quality/public_repo_guard.py` 检查 Git 跟踪内容：

- 禁止 `.private/`、`docs/archive/`、内部评分/TODO/路线图和 host receipt；
- `docs/screenshots/` 只允许公开策略 README；
- 禁止常见私钥、token 和云凭据格式；
- 禁止未允许的邮箱以及真实用户 home/media 路径；
- 文本解码失败或可疑的大型系统快照必须显式处理，不能静默跳过。

正式发布额外使用 history 模式，要求所有历史作者邮箱使用 GitHub noreply，并确认历史对象
中不再包含私有路径。仓库首次公开前必须完成一次历史重写并通过该模式。

## 4. 文档门禁

公开文档遵循 [docs/README.md](README.md) 的路由：

- 每篇 living 文档不超过 200 行，AGENTS 保持短小；
- 本地链接必须存在且不能逃出仓库；
- 每个 crate 必须有 Role、Boundary、Contract and verification；
- 当前文档不能链接历史、评分、TODO 或真实回执；
- 规则变化直接重写正文，不追加日期流水。

## 5. 发布物门禁

tag 发布必须同时构建并验证（命名约定见 PRODUCT_IDENTITY.md，`<ver>` 为 Cargo 版本）：

- `TaskForest-G-<ver>-x64.deb` 与 `TaskForest-G-<ver>-arm64.deb`：`dpkg-deb --info` 和
  `--contents`；
- `TaskForest-G-<ver>-x64.rpm` 与 `TaskForest-G-<ver>-arm64.rpm`：`rpm -qp --info`、
  `--list` 和 `--requires`；
- `TaskForest-G-<ver>-x64.msi` 与 `TaskForest-G-<ver>-arm64.msi`：WiX 构建、MSI 数据库反编译和
  payload 引用检查；
- 每个架构独立的 Linux/Windows SHA-256 清单。

Windows 或 Linux 任一打包 job 失败，整个发布失败。所有必需文件验证完成后，单一 publish
job 才能创建或更新 GitHub Release，避免半成品发布。

## 6. 截图与真实环境

真实主机截图和回执只保存在忽略的 `.private/` 或 `target/` 中。公开截图必须来自确定性
演示数据，并检查用户名、进程、网络、设备、路径和元数据。无真实目标环境时只能报告
SKIP，不能把 fixture、编译或静态图片写成平台验证通过。

捕获后端按能力分类，不得用一个后端的成功替代另一个后端的语义验证：

- nested Niri 已建立 IPC socket，但在客户端映射后 `niri msg` 超时：标记为
  `BLOCKED (compositor/backend)`。这不是产品 PASS，也不把 TaskForest 判为产品 FAIL；保留本地
  证据，不更新 accepted screenshots，等待真实 compositor 或后端修复后重跑。
- gamescope 可以作为单应用、固定输出尺寸的辅助像素捕获后端。只有真实应用 marker、PNG、
  source manifest 和独立验证器全部通过时，结果才覆盖 standalone 渲染与弹性布局审查。
- gamescope 即使广播 `zwlr_layer_shell_v1`，也不能单独证明 layer surface 已正确合成。anchor、
  margin、exclusive zone、keyboard interactivity、output 选择、close/restart 和桌面窗口管理
  仍必须在目标桌面 compositor 上验证；未建立专用、可复现的 layer-shell capture receipt 前，
  gamescope 结果只能报告 `SKIP`，不能冒充 Layer-Shell PASS。

## 7. 前端交互与像素证据

每个前端由同一套合同约束，证据通道按 toolkit 能力选择，全部 fail-closed：

| 前端 | headless 交互矩阵 | 真实像素证据 |
|---|---|---|
| GPUI | `scripts/accept-gpui-interactions.sh`（standard `--with-gui`） | `scripts/capture-niri.sh` / `capture-windows.sh` |
| Iced | crate headless tests + capture matrix | `scripts/capture-iced.sh` |
| TUI | crate headless tests + capture matrix | `scripts/capture-tui.sh` |
| Bevy | `scripts/accept-bevy-interactions.sh`（standard `--with-gui`） | `scripts/capture-bevy.sh`（Wayland-only） |

四个前端的画面证据统一默认 `TM_CAPTURE_NIRI_BACKGROUND=1`：由私有
`dbus-run-session`（无 service activation）和 `kwin_wayland --virtual` 承载 nested Niri，
按真实 PID/app-id/window-id 绑定窗口，使用
Niri `screenshot-window` 写出 PNG；验收脚本拒绝 `TM_CAPTURE_NIRI_BACKGROUND=0`，避免调试
参数意外触碰宿主桌面。需要研究可见 compositor 行为时，必须使用独立的手工调试环境，
不得把结果写入验收 receipt。

每个后台 capture 必须经 `scripts/capture_supervisor.py` 获得随机 Run UUID，并将应用
binary、KWin 的 runtime/config/data/cache/state、Niri socket、D-Bus session 与 receipt
绑定到该 UUID；supervisor 使用用户 cgroup v2 和 detached watchdog 管理完整进程树。
失败、取消或父进程消失都必须留下可审计 receipt 并回收 runtime；`latest` 只能由通过
独立 validator 的完整 run 在 `flock` 下以原子 pointer 发布。并发隔离由
`scripts/test_capture_isolation.py` 证明：A/B 同时运行、窗口互不可见、单独终止 A 不得
影响 B，主机 D-Bus/Wayland/KWin 状态不变，最终两个 run 都无进程和 runtime 残留。
正式 RC7 本地收口必须以 `TM_CAPTURE_ISOLATION_GATE=1 bash scripts/quality/local-gates.sh
standard` 重跑；CI/Rehearsal 若提供真实 Wayland/KWin runner，则同样强制该变量，普通无图形
runner 只能报告环境性 SKIP，不能把它记为隔离 PASS。

Bevy 交互矩阵（`scripts/bevy_interaction_matrix.tsv`）由机械发现驱动：脚本先对 lib 目标做
nextest discovery，矩阵中的每个命名测试必须真实存在，然后完整运行 lib 目标；矩阵之外不
存在"已登记但未运行"的用例。真实像素走嵌套 Niri，validator 对 app_id、PID/窗口身份、PNG、
marker、source provenance 和当前 worktree fail-closed；无 compositor 时只报告 SKIP。
UI 边界改动由 `scripts/quality/ui-evidence-route.sh` 按前端路由到对应矩阵与新鲜回执。

### 7.1 弹性布局收口协议

所有响应式、密集详情、图表分组或固定 viewport 改动必须先遵循
[ELASTIC_LAYOUT_PLAYBOOK.md](ELASTIC_LAYOUT_PLAYBOOK.md)：先从根预算推导完整 slot footprint，
再按 mandatory/elastic/optional 分配；lower band 只能整组准入、摘要或隐藏，底部安全带和
右侧 label/value bounds 必须有 headless 断言。完成后必须运行当前构建的真实 capture 并通过
独立 validator，且逐页检查最小、正常、tall、wide-short、窄宽和长文本状态；单张截图、fixture
或编译通过不得替代这套证据。

## 8. 验证器质量

验证器必须自动发现范围、执行目标、检查结果和副作用，并在范围为空、解析失败或回执不完整
时 fail-closed。禁止用源码字符串存在性、恒真断言、固定测试数量或 `echo PASS` 证明行为。

## 9. 并行隔离验证与视觉对等（标准，2026-08-29 起）

**并行隔离验证**（`scripts/verify-isolated.sh <line> [gate]`，各行通用）：行门禁只在
本行自己的差异上跑——脚本按本行足迹清单（`scripts/isolated-paths/<line>.txt`）把本行
未提交改动（含 intent-to-add 未跟踪文件）应用到 HEAD 私有 worktree 内执行 fmt/clippy/
nextest/capture，主 checkout 永不被验证流程改动。这样兄弟线在共享 checkout 里的未提交
重构（哪怕暂时编译不过）永远不阻塞本行门禁；本行也不得因他行撕裂而宣布 SKIP。足迹清单
是行声明：本行拥有或改动的路径（含共享 crate 的行内改动）必须全部列入，隔离验证才携带
全部本线事实。Cargo 锁回退沿用全仓约定（`TM_CARGO_LOCK` 空值 = 解锁，未设 = `--locked`）。

**视觉对等目检**（渲染行收口门禁）：绿色门禁只证明行为与来源，不证明观感。渲染行每轮
收口必须把本行捕获与参考行捕获逐页并排目检，差距逐条记账——修掉的进提交，暂缓的写入
该行公开文档的"已知边界/存异"清单（如 `docs/BEVY_UI_FRONTEND.md`）。装饰禁止用文本
字形冒充（嵌入字体无该字形即渲染 tofu）；图标走语义注册表（`IconId` → 共享 SVG → 各
toolkit 材质化），行门禁须含机械反字形扫描与有界行单行契约测试。
