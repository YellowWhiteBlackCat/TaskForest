# TaskManager 代码标准与 Rust 2024 规范 (STANDARDS.md)

## 1. Rust 2024 Edition 规范

1. **分层 safe-Rust**（ADR-022/024/025/031，权威记录见 `docs/PERMISSION_MODEL.md` Boundary 1）：
   业务/产品 crate 一律 `#![forbid(unsafe_code)]`；`unsafe` **仅存在于四个审计边界
   crate**——`taskmanager-perf-ioctl`（perf_event_open）、`taskmanager-afpacket`（AF_PACKET）、
   `taskmanager-fd-bridge`（SCM_RIGHTS）、`taskmanager-windows-api`（最小 Windows
   performance/locale/Known Folder/exact-process/WTS/SCM、processor topology/cache、NIC
   metadata API）。每个边界 crate：根带 `#![deny(unsafe_op_in_unsafe_fn)]`（非 `forbid`）、
   每个 `unsafe` 块带 `// SAFETY:` 注释、公开 API 不跨原始 OS handle/pointer（Unix 使用
   `OwnedFd`/`impl AsFd`，Windows 只出 typed 值）、指针加宽只用 `.cast()`（禁
   `as *const`/`as *mut`/`as RawFd`）、零 workspace 依赖。由
   `tests/logic/workspace_architecture_test/dependency_firewall.rs` 逐构建强制（4-crate
   allowlist + 边界契约 + 四道反向防火墙）。
   - **遍历守卫与双执行面纪律**（裁决详版见 [PERMISSION_MODEL](PERMISSION_MODEL.md)
     Boundary 1）：遍历宏按最弱保证审计，每项长度守卫相对当前元素偏移；unsafe 纯逻辑面
     有双执行面（Miri 默认门 + 解析面 fuzz target）。审计是准入，执行面才是持续防线。
   - 严格检查数值转型，禁止无防护的 `as` 强转可能导致溢出的位置，使用 `try_into()` 或
     `saturating_cast`。
2. **错误处理策略**：使用 `thiserror` 或 `std::fmt::Display` 定义强类型枚举 Error。
   核心逻辑层禁止滥用 `unwrap()` / `expect()`，必须向上传递 `Result<T, E>` 或提供合理 Fallback。
3. **Windows native-first 红线（ADR-018/031）**：
   - 先选成熟 safe crate（当前包括 `sysinfo`、`nvml-wrapper`、`windows-registry`、
     `windows-service`、`starship-battery`、`open`）；只有没有成熟封装且 ABI 足够小、
     值得审计时，才能新增独立 boundary crate。
   - native wrapper 必须只暴露 typed 值/错误，内部拥有句柄和编码缓冲；OS 返回的长度须先
     做符号、上界、整数转换检查；分配/枚举/等待/输出必须有上限；每块 `unsafe` 都有
     `SAFETY:` 证明；生产路径拒绝 panic。否则返回 typed `Unsupported`，不伪造 0 或空数据。
   - `tests/logic/workspace_architecture_test/dependency_firewall/frontend_safety.rs` 的
     Windows 负向门扫描完整 adapter 源码，回归即失败。

---

## 2. 代码风格与设计模式

1. **命名规范**：结构体/枚举/Trait `PascalCase`，函数/方法/变量 `snake_case`，常量 `SCREAMING_SNAKE_CASE`。
2. **状态与并发**：
   - UI 线程与后台采集线程解耦，通过 `crossbeam-channel` 通信。
   - 共享只读状态采用 `Arc<T>`；可变状态避免长时间持锁。系统图表统一读取
     `CorrelatedSystemTelemetryHistory`，写能力仅由组合边界持有的独立 ingestor 暴露；
     历史锁区必须保持短小，初始状态不得预填伪造零值。
   - 多个 `bool` 如果共同编码互斥状态、合法组合或迁移关系，必须改为 typed enum/显式状态机
    （如 `Rejected`/`AcceptedPartial`/`Committed`）。独立 dirty flag、能力开关和事实属性
     可继续使用 `bool`，不得借其隐式表达生命周期。

---

## 3. 测试分类标准

测试文件布局以 [`TEST_LAYOUT.md`](TEST_LAYOUT.md) 为硬约束；新代码不得增加内嵌测试。

1. **逻辑测试 (`tests/logic/`)**：独立于 UI/GPU，涵盖数据采集、环形缓冲、进程筛选排序等。
   含 live smoke（经 `platform-native` 跑 1 tick，只断言宿主无关性质）。
2. **GUI 测试 (`tests/gui/`)**：GPUI `TestAppContext` headless 窗口，覆盖页面/皮肤/窗口状态/
   设置/键盘。不依赖真实 compositor 或 GPU；真实像素见 [`screenshots/README.md`](screenshots/README.md)。
3. **统一执行器**：unit/integration 用 `cargo nextest run --locked --workspace --all-targets -j 4`；
   standard 门禁按 core/logic/gui/perf 四层拆分，`--only nextest-core` 可单独复跑。
   Doctest 用 `cargo test --locked --doc --workspace -j 4`；`live-smoke` 为独立 stage。
4. **TUI 测试**：`TestBackend` 覆盖参考尺寸与 54x16 最小尺寸；真实终端证据
   `bash scripts/capture-tui.sh`，不得用纯文本 snapshot 代替像素截图。
5. **测试总原则（八荣八耻 + 五问）**：
   - 总原则：测试价值在于以低长期成本稳定发现真实回归，不在于证明代码被写过。
   - 八荣：保护真实回归 / 验证行为不变量 / 独立 Oracle / 直接验证语义 /
     新增检测能力 / 确定性验证 / 低脆弱低维护 / 最低保证层级。
   - 八耻：证明代码存在 / 锁定实现细节 / 复制实现自证 / 源码文本代理语义 /
     重复覆盖刷率 / `sleep` 时序侥幸 / 正常重构即破坏 / 替代类型编译器 lint。
   - 保证层级：类型系统→不写运行时测试；compiler→不写文本测试；lint→不写源码扫描；
     结构检查→不写脆弱代理；只有真正行为→才进入行为测试。
   - 五问准入：①保护哪个 regression？②什么 mutation 能让它 fail？③无行为重构会不会
     break？④已有覆盖？⑤更直接更稳定的保证方式？答不清①②不得新增；③为"会"时重写。
   - Agent 禁令：禁止为"有测试/提高覆盖率/证明修改存在"新增测试。低价值测试不如没有。
   - **源码文本代理语义红线**：默认禁止测试读取 production source 证明 behavior；允许的
     source inspection 只属 `static-policy`/`source-transformation`/`textual-artifact` 三类，
     文件头须声明（`source_inspection_guard.py` enforce）。canonical 实现上线时同一变更
     删除旧路径/测试/fixture/demo 与文档。正向复读一律禁止。存量须迁移到可执行断言或声明合法类别。
6. **空断言即灌水（禁止）**——`#[test]` 须对真实输出/状态/副作用做断言（`no_test_function_is_assertion_free` 拦截）。
   无效形态：仅 println 无 assert、断言恒真式、只断言 `is_ok()` 不验证副作用、读后丢弃 `let _ = value`。
7. **可移植性红线**：禁止 `.expect("python3 must be available")`（先探测，缺失则 `eprintln!` +
   return）；禁止硬编码宿主值；实时硬件读取须 `#[cfg(target_os)]` 守卫且只断言聚合性质；
   优先纯函数表驱动测试。
8. **枚举变体单一事实源**：遍历枚举全部变体时引用 crate 的 `ALL`/迭代器，不得在测试重抄变体清单。

### 3.5 Rust/Python/Shell 反规避契约

三种实现共享同一标准：证明可观察行为与不变量，不证明"名字出现过"。
- Rust 测试通过 `ALL`/registry/trait/运行时输入自动发现范围，不逐个点名。
- Python 门禁解析真实输入、构建关系或执行目标命令后判定；不把路径/名称逐项抄成"通过清单"。
- Shell 脚本运行真实产物并检查退出码/状态/产物/收据；禁止只 `echo PASS` 或只 `grep` 文本。
- "报菜名"/手工清单/恒真断言/空断言/吞错返回 0 均为灌水；不得以数量或覆盖率替代证据。
- allowlist 只表达真实政策例外且须有原因和失败语义；验证器自身 fail-closed。

9. **平台契约套件**：`platform-conformance` 承载宿主无关断言（capability 表面、process 行、
   live drain 归属），不含 OS I/O 与 `cfg(target_os)` 分支。三平台 adapter 各以
   `conformance.rs` 真机运行同一场景；`live_smoke_test.rs` 经组合边消费同一契约。
   UI 变更的 `--with-gui`/capture 路由见 QUALITY_GATES.md §2.2。
10. **零系统副作用红线**——测试不得弹窗/通知/托盘/打开 URL/调用 `pkexec`/`sudo`/`xdg-open`
    等交互或特权二进制；不得调用 `NativeAppHost::production()` 读真实用户配置；Windows
    子进程须带 `CREATE_NO_WINDOW`（`0x08000000`）；GUI 证据只经显式 capture 流程产生，
    `cargo nextest` 默认路径不得启动真实事件循环。测试临时文件统一 `repo_temp_dir()`
   （仓库 `.tmp/`）。`headless_side_effect_guard.py` enforce。

---

## 4. 功能与 UI 证据门禁

功能或可见 UI 改动须同时提交四类证据（命令/矩阵/回执详见 [`screenshots/README.md`](screenshots/README.md)
与 [`QUALITY_GATES.md`](QUALITY_GATES.md)）：

1. **无头行为测试**：覆盖成功/失败/取消路径，断言状态或输出（不能只证明"没有崩溃"）。
2. **可观测埋点**：结构化 tracing marker/状态转储/等价探针；须可在 capture/test 模式开启，
   不记录隐私/机密/用户文件内容。
3. **真实像素截图**：按前端 capture 脚本运行，须来自本次构建和真实渲染帧；旧图/mock 图/
   仅构造 Scene 不能替代。
4. **截图审查记录**：逐项检查信息层级/密度/间距/对齐/截断/对比度/空白利用/加载&空&错误状态；
   按风险覆盖亮暗主题/最小窗口/缩放/键盘路径。

回执新鲜度只属 capture/`--with-gui` 证据流程，默认 nextest 不得对已提交回执做哈希比对。
每次截图运行应保存 git commit/工作树标识/Rust 版本/场景参数/埋点日志/PNG 尺寸哈希
及成功失败清单；未生成这些记录的运行只算人工预览，不能作为验收证据。

---

## 5. 文件规模与职责门禁

拆分按可命名职责或数据边界，使用 `foo.rs + foo/` 模块树；禁止 `part_2.rs`/`misc.rs`/泛化
`helpers.rs`/`include!`/机械 re-export。达到 650 行不得继续添加逻辑；CI `--mode enforce`
硬阻断，不能用 warning/allowlist 或报告备注延后切分。

函数职责由 Clippy 全 workspace 收紧（`cognitive_complexity` 上限 48、单函数非注释行
上限 650，`-D warnings`；阈值是存量棘轮，不是新代码目标）。新增交互/更新/渲染编排应拆成
命名 system（单决策函数复杂度 ≤25），不得用 `#[allow]`/转发壳/换文件规避。
`rust_surface_guard.py` 阻断 `*_open: bool`/`*_was_open: bool`，须改为 typed surface
或穷尽 presence transition。

---

## 6. 自动化活性与进程生命周期

Rust 类型/内存安全不证明任务一定结束；Python/Shell/Rust 子进程须有可验证活性边界。

1. **脚本可审查**：含循环/递归/进程启动/超约 20 行逻辑须提交 `scripts/` 并带自测，禁
   `python3 - <<...` heredoc 复杂临时代码。
2. **循环证明前进**：优先 `for`/`Path.parents`/固定次数/有界队列；`while` 至少一个变量在
   循环体内直接推进，禁无限循环依赖"通常会 break"。
3. **双层截止时间**：Python `subprocess.run(timeout=N, check=True)`；Shell/CI 再用
   `timeout --kill-after` 包裹。
4. **精确拥有回收**：后台进程立即保存 `$!`/独立 PGID，首个子进程前安装唯一 EXIT cleanup；
   INT/TERM 经 cleanup 退出。清理先 TERM 有限等待再 KILL 兜底并 wait。禁 `pkill`/`killall`。
5. **失败后检查残余**：长任务结束后按 PID/PPID/PGID/运行时间/CPU/cwd 审计残余；
   未确认工作目录和祖先前不得终止进程。

统一门禁：`timeout 30s python3 scripts/quality/automation_safety_guard.py`（`--self-test`
自证）；执行入口见 [`QUALITY_GATES.md`](QUALITY_GATES.md)。
