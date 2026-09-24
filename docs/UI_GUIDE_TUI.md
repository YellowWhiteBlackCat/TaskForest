# TUI (TaskForest-T) 前端架构与开发指南

本文定义 `taskmanager-tui` 前端的字符网格模型、终端无头渲染与键盘优先交互纪律。
跨端中立契约见 [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md)。

## 1. 核心思维：终端字符网格与状态快照驱动

- **全屏字符网格约束**：TUI 基于 Ratatui 与 Crossterm 构建，其物理表面是由行与列
  构成的离散字符单元（Cell），不支持像素级坐标与任意定位。
- **单帧原子重绘与零分配**：每一帧通过 `terminal.draw(|f| ...)` 进行整屏重绘；
  热路径上禁止在渲染循环内分配动态内存，重用 `FramePlan` 与字符缓冲区，确保运行期
  内存稳定在 ~18MB 的极低水平。
- **存异思维：键盘为先（Keyboard-First）**：TUI 放弃复杂的鼠标指针依赖，
  交互全部通过纯键盘和弦（`Alt+1..7` 切页、`Tab` 切换焦点、`s` 排序循环、
  `Space` 多选标记、`Delete` 结束任务）单向驱动。

## 2. 布局与比例约束划分

- **声明式分块（`Layout` / `Constraint`）**：页面结构通过 Ratatui `Layout`
  严格按垂直与水平比例分割：
  - 顶部：全局快捷键提示与页面指示器（1 行）；
  - 中部：主工作区（表格或分栏图表）；
  - 底部：状态栏与操作反馈行（1 行）。
- **紧凑模式退化**：当终端行数或列数不足（如低于 80×24 字符）时，自动触发
  `empty.terminal_too_small` 友好警告提示，引导用户缩放终端，杜绝视口溢出崩溃。

## 3. 字符级绘图与盲文 Canvas (Braille Sparkline)

- **Unicode 盲文字符高精度走势图**：在终端无像素绘图能力的限制下，利用 Unicode
  盲文点阵（Braille Patterns, U+2800..U+28FF，每个字符包含 2×4 = 8 个子点），
  在单字符空间内实现 4 倍纵向分辨率与 2 倍横向分辨率的高平滑度 CPU 历史走势。
- **诚实呈现**：采集中或未支持的标量如实渲染 `—` 破折号占位符，绝不伪造零值或跳变。

## 4. 模态、帮助与快捷操作

- **单层模态覆盖**：属性面板（Properties）、确认门（Confirm Modal）与
  键盘帮助（Help）以居中悬浮框（`Clear` + `Block`）的形式呈现，`Esc` 键始终
  作为第一优先级的退出退出通道。
- **无头测试与验收矩阵**：通过 `scripts/accept-tui-interactions.sh`
  执行 600+ 项无头终端虚拟会话测试，自动验证按键序列、状态机响应与 ANSI 转义序列。

## 5. 锚定证据边界

TUI 通过 `ratatui::backend::TestBackend` 回读真实绘制帧文本，Swap 单元格锚点本身
即绘制帧断言，无需额外能力。
