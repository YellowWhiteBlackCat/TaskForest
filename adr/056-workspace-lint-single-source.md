# ADR-056: 单一 lint 事实源（workspace.lints）与四个 unsafe 信任根例外

Status: accepted

- 相关：[ADR-022](022-audited-perf-boundary-crate.md)、
  [ADR-024](024-afpacket-boundary-crate.md)、
  [ADR-025](025-fd-bridge-scm-rights-boundary-crate.md)、
  [ADR-031](031-windows-native-safe-boundary.md)、
  [PERMISSION_MODEL.md](../docs/PERMISSION_MODEL.md) Boundary 1、
  [STANDARDS.md](../docs/STANDARDS.md) §2

## 背景

仓库的 lint 管控此前完全散落在源码内联属性里：每个业务 crate 根带
`#![forbid(unsafe_code)]`，模块内另有模块级副本，四个 unsafe 信任根 crate 根带
`#![deny(unsafe_op_in_unsafe_fn)]`。这种做法的失效模式是"新 crate 漏写"：管控强度
取决于每个 crate 作者的记忆，没有一份可审计的清单，也没有一处能回答"这条 lint 在
哪里被放宽"。

Cargo 的 `[lints]` 机制提供工作区级事实源，但有两条硬约束决定了它能怎么用：

- `[lints] workspace = true` 与同一张表内的其它 lint 键**互斥**。成员 crate 只能
  整表继承，不能"继承后再局部放开"；一个 crate 若需要放宽某条 lint，必须放弃
  继承并自己写完整表。
- `forbid` 不能被低层 `allow` 放松，`deny` 可以。crate 级
  `#![forbid(unsafe_code)]` 与清单级 `deny` 因此不是同一条规则的两种写法：前者是
  "本 crate 内任何位置都不可放开"，后者是"默认拒绝，可被显式登记的例外覆盖"。

unsafe 的分布另有一条既有铁律：只有四个审计边界 crate 允许出现 `unsafe`
（ADR-022 perf-ioctl、ADR-024 afpacket、ADR-025 fd-bridge、ADR-031 windows-api）。
集成测试目标是独立 crate，不受库 crate 根属性约束，因此需要一条显式的例外登记
通道，而不是让边界从"四个"退化成"四个加若干隐式例外"。

## 决策

1. **根清单是唯一 lint 事实源**：根 `Cargo.toml` 的 `[workspace.lints.rust]` 声明
   `unsafe_code = "deny"`、`unsafe_op_in_unsafe_fn = "deny"`、
   `unused_must_use = "deny"`。业务 crate 一律用 `[lints] workspace = true` 继承，
   并保留自己的 crate 级 `#![forbid(unsafe_code)]`——清单保证"不会漏"，源码属性
   保证"不能就地放开"。
2. **四个 unsafe 信任根显式声明例外**：Cargo 不允许这些 crate 继承后放宽，因此它们
   各自携带显式 `[lints.rust]`：`unsafe_code = "allow"`、
   `unsafe_op_in_unsafe_fn = "deny"`、`unused_must_use = "deny"`。这四张表是工作区内
   **全部**的 lint 例外，不得新增第五个 unsafe 信任根；新增边界必须先有新的 ADR。
3. **测试期 unsafe 必须显式登记**：集成测试目标独立编译，能绕开库 crate 根的
   `forbid`，因此每个测试期 `unsafe` 站点必须在文件顶部写**作用域豁免**并附理由。
   当前登记两处，都是进程级分配计数（`unsafe impl GlobalAlloc`）：

   - `crates/taskmanager-tui/tests/headless/perf_budget_alloc_tests.rs`：TUI 每帧
     分配预算；
   - `tests/performance.rs`：采集循环的保留内存预算（豁免按
     `all(test, target_os = "linux")` 收窄到真正安装分配器的平台）。

   两处都只链接库的公开 API、不随产品发布；库本身仍是 `#![forbid(unsafe_code)]`。
4. **豁免写法**：工作区唯一被认可的豁免形态是 `cfg_attr(...)` 作用域式
   （`#![cfg_attr(test, allow(lint))]`）——`scripts/quality/allow_ceiling_guard.py`
   对裸 `#[allow(...)]` / `#[expect(...)]` 一律判违规，且审计普查为零；要新增豁免
   必须走 `cfg_attr`、修掉根因，或在同一变更里登记进该守卫的 `CEILINGS`。
   每条豁免必须写明理由并与源码同处，禁止整 crate 大范围放开。

## 影响

- 新增 crate 声明 `[lints] workspace = true` 后自动继承三条 lint；漏写继承声明
  不会被编译器发现，因此工作区成员清单与继承声明的一致性必须由结构门禁核对。
- 四个信任根的例外是显式且可审计的：放宽面收敛到四张三行清单，审计者不必在源码
  里搜索 `allow`。
- 测试期 unsafe 从"隐式事实"变成"登记项"：任何新的计数分配器类需求必须先在例外
  表登记，登记面因此保持可枚举。
- `unused_must_use = "deny"` 让被丢弃的 `Result` 在编译期暴露；它与
  `[build] warnings = "deny"`（ADR-058）叠加后不留静默降级。

## 验证

- 工作区每个成员要么继承 workspace lint 表，要么是登记在案的四个信任根之一；
  继承声明的完整性由结构门禁核对，不依赖人工记忆。
- 三个 Linux 信任根继续由 `scripts/quality/miri-boundaries.sh` 覆盖（Undefined
  Behavior 即失败），Windows 信任根由 Windows 目标编译、clippy 与原生/契约测试
  覆盖；逐块 `// SAFETY:` 审计面不变。
- 除登记的测试例外外，工作区不存在其它 `unsafe` 站点；`cargo clippy --workspace
  --all-targets` 在三平台上无 lint 逃逸。
