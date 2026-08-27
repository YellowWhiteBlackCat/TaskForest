# TaskManager 代码标准与 Rust 2024 规范 (STANDARDS.md)

## 1. Rust 2024 Edition 规范

1. **模块声明规则**:
   - 禁止在代码中使用旧版 `foo/mod.rs` 文件结构。
   - 所有子模块必须使用同名文件作为入口（例：`crates/taskmanager-core/src/core.rs` 管理 `crates/taskmanager-core/src/core/metrics.rs` 等）。
2. **显式类型与安全约束**:
   - **分层 safe-Rust**（ADR-022/024/025/031，权威记录见 `docs/PERMISSION_MODEL.md` Boundary 1）：业务/产品 crate 一律 `#![forbid(unsafe_code)]`；`unsafe` **仅存在于四个审计边界 crate**——`taskmanager-perf-ioctl`（perf_event_open）、`taskmanager-afpacket`（AF_PACKET）、`taskmanager-fd-bridge`（SCM_RIGHTS）、`taskmanager-windows-api`（最小 Windows performance/locale/Known Folder/exact-process/WTS/SCM、processor topology/cache、NIC metadata API）。每个边界 crate：根带 `#![deny(unsafe_op_in_unsafe_fn)]`（非 `forbid`）、每个 `unsafe` 块带 `// SAFETY:` 注释、公开 API 不跨原始 OS handle/pointer（Unix 使用 `OwnedFd`/`impl AsFd`，Windows 只出 typed 值）、指针加宽只用 `.cast()`（禁 `as *const`/`as *mut`/`as RawFd`）、零 workspace 依赖。由 `tests/logic/workspace_architecture_test/dependency_firewall.rs` 逐构建强制（4-crate allowlist + 边界契约 + 四道反向防火墙）。
   - 严格检查数值转型，禁止无防护的 `as` 强转可能导致溢出的位置，使用 `try_into()` 或 `saturating_cast`。
   - **边界 unsafe 的遍历守卫与双执行面纪律**（裁决详版见 [PERMISSION_MODEL](PERMISSION_MODEL.md)
     Boundary 1）：遍历宏按最弱保证审计，
     每项长度守卫相对当前元素偏移；unsafe 纯逻辑面有双执行面（Miri 默认门 + 解析面 fuzz
     target）。审计是准入，执行面才是持续防线。
3. **错误处理策略**:
   - 使用 `thiserror` 或标准库 `std::fmt::Display` 定义强类型枚举 Error。
   - 核心逻辑层禁止滥用 `unwrap()` / `expect()`，必须向上传递 `Result<T, E>` 或提供合理的 Fallback。

4. **Windows native-first 红线（ADR-018/031）**:
   - Windows 生产代码、测试和开发辅助代码禁止 PowerShell 或其它命令解释器承担遥测。
   - 先选成熟 safe crate（当前包括 `sysinfo`、`nvml-wrapper`、`windows-registry`、
     `windows-service`、`battery`、`open`）；只有没有成熟封装且 ABI 足够小、值得审计时，
     才能新增独立 boundary crate。
   - native wrapper 必须只暴露 typed 值/错误，内部拥有句柄和编码缓冲；OS 返回的长度须先
     做符号、上界、整数转换检查；分配/枚举/等待/输出必须有上限；每块 `unsafe` 都有
     `SAFETY:` 证明；生产路径拒绝 panic。否则返回 typed `Unsupported`，不伪造 0 或空数据。
   - `tests/logic/workspace_architecture_test/dependency_firewall/frontend_safety.rs` 的
     Windows 负向门扫描完整 adapter 源码，回归即失败。

---

## 2. 代码风格与设计模式

1. **命名规范**:
   - 结构体 / 枚举 / Trait: `PascalCase`（例 `MetricsCollector`）
   - 函数 / 方法 / 变量: `snake_case`（例 `fetch_process_snapshot`）
   - 常量: `SCREAMING_SNAKE_CASE`（例 `MAX_RING_BUFFER_SIZE`）
2. **状态与并发**:
   - UI 线程与后台采集线程解耦，通过 `crossbeam-channel` 通信。
   - 共享只读状态采用 `Arc<T>`；可变状态避免长时间持锁。系统图表统一读取 `CorrelatedSystemTelemetryHistory`，写能力仅由组合边界持有的独立 ingestor 暴露；历史锁区必须保持短小，初始状态不得预填伪造零值。
   - 多个 `bool` 如果共同编码互斥状态、合法组合或迁移关系，必须改为 typed enum/显式状态机；例如 `Rejected`、`AcceptedPartial`、`Committed` 不得拆成 `accepted` 与 `snapshot_committed` 两个布尔字段。彼此独立的 dirty flag、能力开关和事实属性可以继续使用 `bool`，但不得借它们隐式表达生命周期。

---

## 3. 测试分类标准

测试文件布局以 [`TEST_LAYOUT.md`](TEST_LAYOUT.md) 为硬约束：每个 crate 的 `src/` 只放
生产代码，测试统一进入 `tests/common*`、`tests/headless*` 或 `tests/gui*`。迁移期间
允许极少量 cfg 挂载声明，但不得在生产文件内定义测试函数、测试模块、fixture 或测试
专用实现；新代码不得增加任何内嵌测试。

1. **逻辑测试 (`tests/logic/`)**:
   - 必须独立于 UI 渲染层与 GPU 驱动环境。
   - 涵盖数据采集精度、环形缓冲区推入推导、进程筛选排序等。
   - 含真实采集 live smoke（`tests/logic/live_smoke_test.rs`）：经
     `taskmanager-platform-native` 组合边跑 1 tick，只断言宿主无关性质。
2. **GUI 自动化测试 (`tests/gui/`)**:
   - 使用 GPUI `TestAppContext` 构造 headless 窗口与 `RootView`。
   - 覆盖页面/皮肤渲染、窗口状态、设置切换与键盘分发。
   - 不依赖真实 Wayland compositor 或 GPU 设备；真实像素证据按前端流程由 [`screenshots/README.md`](screenshots/README.md) 补充。
3. **统一执行器**:
   - Unit/integration 测试使用 `cargo nextest run --locked --workspace --all-targets`；
     standard 门禁按 `nextest-core`/`nextest-logic`/`nextest-gui`/`nextest-perf` 四层拆分，
     逐 stage 记录，`--only nextest-core` 可自底向上单独复跑。
   - Doctest 使用 `cargo test --locked --doc --workspace` 单独执行；nextest 不包含 doctest。
   - `live-smoke` 是 standard 独立 stage，只跑真实采集冒烟测试。
4. **TUI 自动化**:
   - Ratatui 页面使用 `TestBackend` 覆盖参考尺寸与 54×16 最小尺寸；运行 `cargo nextest run --locked -p taskmanager-tui`。
   - 真实终端证据运行 `bash scripts/capture-tui.sh`，不得用纯文本 snapshot 代替像素截图。
5. **测试总原则（八荣八耻 + 五问）**:
   - 总原则：测试的价值，不在于证明代码被写过，而在于以尽可能低的长期成本，稳定地发现
     值得防止的真实回归。
   - 八荣八耻：
     1. 以保护真实回归为荣，以证明代码存在为耻。
     2. 以验证行为与不变量为荣，以锁定实现细节为耻。
     3. 以独立 Oracle 为荣，以复制实现自证为耻。
     4. 以直接验证语义为荣，以源码文本代理语义为耻。
     5. 以新增检测能力为荣，以重复覆盖、刷覆盖率为耻。
     6. 以确定性验证为荣，以 `sleep`、时序侥幸和 flaky 为耻。
     7. 以低脆弱、低维护成本为荣，以正常重构即破坏测试为耻。
     8. 以最低且最直接的保证层级为荣，以测试替代类型、编译器、lint 和静态分析为耻。
   - 保证层级：能由类型系统保证 → 不写运行时测试；能由 compiler 保证 → 不写文本测试；
     能由 lint 保证 → 不写源码扫描测试；能由结构检查保证 → 不写脆弱行为代理；
     只有真正的行为 → 才进入行为测试。
   - 新增测试五问（新增任何 test case 前必须回答）：
     1. 它保护哪个具体 regression？
     2. 什么错误 mutation 能让它失败？
     3. 正确的无行为重构会不会让它失败？
     4. 现有测试或其他机制是否已经覆盖这个 regression？
     5. 是否存在更直接、更稳定、更便宜的保证方式？
   - 准入规则：答不清第 1、2 问，不得新增测试；第 3 问为"会"时原则上重写；
     第 4 问为"是"时必须证明新增边际价值；第 5 问为"是"时优先使用更低成本机制。
   - Agent 禁令：禁止为了"有测试""提高覆盖率""证明本次修改存在"而新增测试。
     允许零新增测试；低价值测试不如没有测试。
   - 机械红线——源码文本代理语义：默认禁止测试通过读取 production source code 证明
     production behavior；允许的 source inspection 只属于 `static-policy` /
     `source-transformation` / `textual-artifact` 三类，文件头必须声明
     （`scripts/quality/source_inspection_guard.py` enforce）。其他情况转换为
     behavior / structural / compile / integration 测试；若某不变量只能靠正向源码
     contains 证明，先把不变量落进类型或数据模型（typed registry、`ALL` 单一事实源、
     trait 关联、编译期表格）再测可执行形式。负向禁入只适合长期安全/依赖政策边界，不是
     迁移完成证据；canonical 实现上线时必须在同一变更删除或更新旧生产路径、测试、fixture、
     demo/capture 与文档，不能留给负向门或后续维护者收尾。正向复读一律禁止。
   - 存量同类测试必须迁移到可执行断言或声明合法类别；覆盖率可以暂时不足，但不能用
     字符串断言凑数。新增测试必须能证明真实语义，例如 Unknown→"—"、实测零保留、
     typed unavailable 不回放 legacy 值。迁移类改动以行为测试锁定语义，并在同一变更清空
     旧路径；只有确属长期政策边界时才另设自动发现范围的负向门，不写正向 contains 清单。
6. **空断言即灌水（禁止）**——`#[test]` 函数必须对真实输出/状态/副作用做断言，以下形态视为无效覆盖，新增即阻断（`tests/logic/quality_gate_test.rs` 的 `no_test_function_is_assertion_free` 会拦）：
   - 仅 `println!`/`eprintln!` 而无任何 `assert*`（曾经的 `test_cpu_cache_detection`——零断言；真实覆盖在 `hardware_data.rs`）。
   - 断言被测代码之外的恒真式：`assert_ne!(Some(true), Some(false))`（测的是 `Option<bool>` 的派生 `PartialEq`，不是业务契约）、`assert!(100.0 >= 100.0)`（对任意 f64 恒真）。正确做法：断言真实 wire 契约（serde 往返保真）或派生计算（`used_percentage()`）。
   - 只断言 `result.is_ok()` 而不验证副作用（pause/resume 只看返回值、不验证进程真的停了/恢复了）。对有可观测副作用的操作，必须断言效果本身（如轮询 `/proc/<pid>/stat` 状态位 `'T'`）。
   - 读取后立即丢弃 `let _ = value;`（F5 测试读了进程数又丢弃）。要么断言、要么删掉读取。
7. **可移植性红线**——测试在任意 Linux CI runner / 最小容器上结果必须一致：
   - 禁止对外部工具 `.expect("python3 must be available")`（`Command::new("timeout"/"python3")`）。先探测工具是否可用，缺失则 `eprintln!` 记录 + 提前 `return`（判通过），不得 panic；工具存在时仍跑真实校验（参考 `capture_evidence_test.rs::external_validator_available`）。
   - 禁止硬编码宿主值（具体 PID、MHz、KiB、磁盘型号、MAC、挂载点）。需要阈值时从 sysfs 现读，或只断言宿主无关的聚合性质（`> 0`、单调、`>= 单实例`）。
   - 实时硬件读取须 `#[cfg(target_os = "linux")]` 守卫，且只断言宿主无关的聚合性质（参考 `hardware_data.rs`）。
   - 优先纯函数表驱动测试（输入→精确输出），如 `hardware_test.rs` 的 `physical_disk_key` 矩阵——零宿主依赖、最强可移植性。
8. **枚举变体单一事实源**——测试需要遍历某枚举的全部变体时，引用 crate 暴露的 `ALL`/迭代器（如 `EscalationFeature::ALL`），**不得在测试里重抄一份变体清单**。两份清单必然漂移：escalation 门禁曾硬编码 5/7 变体，漏掉 `MemorySmbios`、`PackagePowerRapl` 两个特性却仍绿灯（典型漏报）。新增变体只改 `ALL`，门禁与内联测试自动跟随。

### 3.5 Rust 测试、Python 门禁与 Shell 脚本的反规避契约

三种实现共享同一标准：证明可观察行为与不变量，不证明“名字出现过”。

- Rust 测试必须执行行为、状态、输出、副作用或 mutation；通过枚举 `ALL`、registry、trait 或运行时输入自动发现范围，不逐个点名函数、文件、变体或测试用例。
- Python 门禁必须解析真实输入、构建关系或执行目标命令后判定；仓库元数据可以定义边界，不能把预期路径/名称逐项抄成“通过清单”。
- Shell 脚本必须运行真实产物并检查退出码、状态、产物和收据；禁止只 `echo PASS`、只 `grep` 文本或列出命令后宣称完成。
- “报菜名”、手工清单、恒真断言、空断言、无效 fixture、吞错后返回 0 均视为灌水；不得以测试数量、覆盖率或清单长度替代证据。
- allowlist 只能表达真实政策例外或输入边界，必须有原因和失败语义；不得用 allowlist 覆盖未检查项。
- 验证器自身也必须 fail-closed：范围为空、目标缺失、解析失败、命令未执行或收据不完整时失败或明确跳过，不得伪造通过。

9. **平台契约套件**:
   - `taskmanager-platform-conformance` 承载宿主无关断言（capability 表面、process 行、
     live drain 归属），不含 OS I/O 与 `cfg(target_os)` 分支。
   - 三平台 adapter 各以 `tests/conformance.rs` 在真机运行同一套场景；根包
     `tests/logic/live_smoke_test.rs` 经组合边消费同一契约。
   - UI 边界变更的 `--with-gui` / capture 回执路由见 `QUALITY_GATES.md` §2.2。

10. **零系统副作用红线**——自动化测试不得向用户系统倾倒任何 OS 可见垃圾：
   - 不得弹出真实窗口、对话框、通知、托盘项，不得打开 URL/文件管理器，不得调用
     `pkexec`/`sudo`/`xdg-open`/`notify-send`/`zenity`/`kdialog` 等交互或特权二进制；
   - 不得创建真实托盘图标或发送桌面通知（`spawn_tray`/`notify-rust`），不得调用
     `NativeAppHost::production()` 读取真实用户配置/历史路径；
   - Windows 上任何子进程 spawn（生产与测试同规则）必须带
     `CREATE_NO_WINDOW`（`0x08000000`），禁止闪出控制台窗口；
   - GUI 证据只能通过显式 capture 流程（`--with-gui`/截图脚本）产生，绝不允许
     `cargo nextest` 默认路径启动真实事件循环或真实终端；
   - 测试临时文件必须自建自清，优先仓库 `.tmp/`，不得向系统目录倾倒垃圾。
     测试代码禁止调用 `std::env::temp_dir()`，统一使用测试二进制根模块提供的
     `repo_temp_dir()`（仓库 `.tmp/` 下每进程唯一目录）。
     由 `scripts/quality/headless_side_effect_guard.py` 以 enforce 模式机械拦截。

---

## 4. 功能与 UI 证据门禁

功能或可见 UI 改动必须同时提交以下四类证据，缺任一项不得标记完成。具体命令、矩阵和
回执位置以 [`docs/screenshots/README.md`](screenshots/README.md) 与
[`docs/QUALITY_GATES.md`](QUALITY_GATES.md) 为唯一口径：

1. **无头行为测试**：用 unit/logic/GPUI headless test 覆盖成功、失败及取消路径；测试必须断言状态或输出，不能只证明“没有崩溃”。
2. **可观测埋点**：为目标场景提供结构化 tracing marker、状态转储或等价探针，证明真实应用已经到达待验证状态。埋点须可在 capture/test 模式开启，不记录隐私、命令行机密或用户文件内容。
3. **真实像素截图**：按前端运行唯一 capture 流程（GPUI `capture-niri.sh`、Iced `capture-iced.sh`、TUI `capture-tui.sh`），截图必须来自本次构建和真实渲染帧；旧图、mock 图及仅构造 GPUI Scene 均不能替代。
4. **截图审查记录**：逐项检查信息层级、密度、间距、对齐、截断、对比度、空白利用、加载/空/错误状态和危险操作文案；按改动风险覆盖亮/暗主题、最小窗口、缩放和键盘路径。

回执新鲜度（提交的 source-manifest 哈希与当前生产源码的比对）只属于 capture /
`--with-gui` 证据流程，由截图脚本在本次捕获时生成并校验；默认 `cargo nextest` 不得对
已提交回执做哈希比对——否则每次生产源码变动都会让普通测试变红，直到重跑一次真机截图。

每次截图运行应保存 git commit/工作树标识、Rust 版本、场景参数、埋点日志、PNG 尺寸/哈希及成功失败清单。截图工具若没有生成这些记录，该次运行只能算人工预览，不能作为验收证据。

---

## 5. 文件规模与职责门禁

采用非空、非注释代码行口径：生产 Rust 文件达到 650 行预警，超过 1200 行阻断；测试 Rust 文件超过 999 行阻断。统一入口为 `python3 scripts/quality/rust_line_guard.py`。

拆分必须按可命名职责或数据边界进行，使用 `foo.rs + foo/` 的真实模块树；禁止 `part_2.rs`、`misc.rs`、泛化 `helpers.rs`、`include!` 或机械 re-export 隐藏耦合。超限文件被触碰时必须优先缩减；CI 以 `--mode enforce` 阻断硬超限，预警保留在报告中供后续拆分审查。

函数职责由 Clippy 全 workspace 统一收紧（`cognitive_complexity` 上限 48、单函数非注释行
上限 650，`-D warnings` 阻断；阈值是存量棘轮，不是新代码目标）。新增或触碰交互/更新/渲染
编排时应拆成命名 system（单决策函数复杂度 ≤25），不得用 `#[allow]`、转发壳或换文件规避。
生产契约中的 `*_open: bool` / `*_was_open: bool` 由
`rust_surface_guard.py` 阻断，必须改为 typed surface 或穷尽的 presence transition。

---

## 6. 自动化活性与进程生命周期

Rust 的类型和内存安全不能证明任务一定结束；Python、Shell 与 Rust 子进程都必须具有可验证的活性边界。

1. **脚本必须可审查**：包含循环、递归、进程启动或超过约 20 行的逻辑必须提交为 `scripts/` 文件并带自测，禁止用 `python3 - <<...` heredoc 执行复杂临时代码。
2. **循环必须证明前进**：优先使用 `for`、`Path.parents`、固定次数重试或有界队列。`while` 条件中的至少一个变量必须在循环体内直接推进；禁止依赖“通常会 break”的无限循环。
3. **子进程采用双层截止时间**：Python 使用 `subprocess.run(..., timeout=N, check=True)`；Shell/CI 对构建、验证器等可能阻塞的 workload 再用 `timeout --kill-after=...` 包裹。
4. **精确拥有并回收进程**：后台进程立即保存 `$!` 或独立 PGID，在启动首个子进程前安装唯一的 EXIT cleanup；INT/TERM 明确退出并经过该 cleanup。清理先发 `TERM`、有限等待，再用 `KILL` 兜底并 `wait` 回收。禁止 `pkill`/`killall` 这类可能误伤用户会话的全局名称匹配。
5. **失败后检查残余**：长任务、测试、coverage、Niri/终端截图结束后，按 PID、PPID、PGID、运行时间、CPU 和 cwd 审计残余；在未确认工作目录和祖先进程前不得终止进程。

统一门禁为 `timeout 30s python3 scripts/quality/automation_safety_guard.py`（`--self-test`
自证）：拒绝无可证明推进的 Python `while`、无 timeout 的 subprocess、`Popen`/`os.system`、
内联 Python heredoc、全局名称杀进程，以及缺少 EXIT trap 或 `$!` 所有权的 Shell 后台
任务；执行入口见 [`docs/QUALITY_GATES.md`](QUALITY_GATES.md)。
