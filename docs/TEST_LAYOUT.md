# Rust 测试目录规范

## 目标

每个 workspace crate 的 `src/` 只包含生产代码。测试代码、测试 fixture、测试专用
装配和测试辅助 API 不得与生产 `.rs` 文件的实现糅合。

## 固定布局

```text
crates/<crate>/
├── src/                  # production only
└── tests/
    ├── common.rs         # 公共装配入口
    ├── common/            # fixtures、fake provider、assertion helpers
    ├── headless.rs       # 无 GUI 测试入口
    ├── headless/          # unit-like/integration/headless behavior tests
    ├── gui.rs            # 需要 GUI/窗口的测试入口
    └── gui/               # GPUI/Iced/TUI/Bevy window/render interaction tests
```

`foo.rs + foo/` 是唯一模块形状；禁止 `mod.rs`。某一类没有测试时可以不创建对应
入口，但已有测试必须归入 `common`、`headless` 或 `gui` 之一。

## 分类边界

- `common` 只装配共享 fixture、fake、builders 和断言工具，不直接承担测试用例。
- `headless` 不创建窗口、不依赖 compositor、GPU 或真实 GUI toolkit runtime；纯逻辑、
  contract、provider fixture、runtime replay 和 live smoke 均归这里。
- `gui` 明确创建窗口或执行渲染/键盘/可访问性交互；真实像素证据仍按截图脚本运行。

`headless` 与 `gui` 自动化测试一律零系统副作用：不弹真实窗口/对话框/通知/托盘，不
调用交互或特权二进制，不启动真实事件循环或真实终端；只有显式 capture 流程
（`--with-gui`/截图脚本）才允许产生 OS 可见 UI。测试不得调用
`NativeAppHost::production()`（真实用户配置/历史路径）；Windows 子进程 spawn 必须
`CREATE_NO_WINDOW`，禁止控制台闪窗。临时文件自建自清，优先仓库 `.tmp/`。机械门禁见
`scripts/quality/headless_side_effect_guard.py`（enforce）。

## 迁移与门禁

迁移期间按 `core → application → runtime/platform → shell → frontend` 顺序推进，保持
行为、失败、取消和证据回归。生产源码不得出现内嵌 `#[test]`、`mod tests {}` 或测试
fixture；只允许保留指向 `tests/` 的最小 cfg 挂载声明，最终应由独立测试入口完成装配。

挂载位置同时决定 fixture 可达性：经 `#[path]` 挂进 lib 的 unit 测试**不得消费
`taskmanager-test-support`**（dev-cycle 会让 lib 测试二进制链接两个 core 实例，实测
E0308）；需要共享 fixture 的测试必须落在独立 harness（`tests/headless.rs` 等），dev
依赖边被 dependency firewall 豁免（firewall 只追踪生产依赖；application 为既有先例）。

**上位规则：默认禁止测试通过读取 production source code 来证明 production
behavior**；禁止的是"用源码文本存在性代替软件语义验证"这个行为，而不是某个文本 API。

测试文件头必须声明用途：

- `test-intent: behavior|structural|compile|integration`：可执行测试，禁止读取生产源码。
- 读生产源码的测试必须额外声明 `source-inspection: static-policy|source-transformation|
  textual-artifact`，且只允许这三类；未声明即违规（默认禁止）。

允许的 source inspection 仅限：

- `static-policy`：crate 属性、cfg/feature 边界、依赖方向、legacy/危险 token 禁入等
  以源码文本为载体的静态策略；
- `source-transformation`：生成器、格式化器、compile_fail fixture 等以源码为输入或
  输出的转换契约；
- `textual-artifact`：locale 目录、SVG 资产、capture 回执等文本本身就是契约的产物。

其他情况一律转换为 behavior / structural / compile / integration 测试，由
`scripts/quality/source_inspection_guard.py` 以 enforce 模式机械拦截未声明文件。
迁移完成后，生产 `src/` 扫描必须对 test marker 为零，三类测试入口必须能由 nextest
独立选择运行。
