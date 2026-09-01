# 弹性布局设计与收口协议

本协议是所有可见布局改动的执行流程。它把“窗口变小仍然可读、最后一行不被吃掉、信息不越界”
变成设计前置条件，而不是截图发现问题后的局部补丁。适用于 GPUI，也作为 Iced、TUI、Bevy
页面适配时的 toolkit-neutral 检查表。

## 硬合同

- 先明确滚动 owner。Performance 只有左侧设备选择栏允许滚动；主视图、右侧统计栏和固定页
  面内容不得挂载滚动句柄。其他页面若允许滚动，必须在页面合同中明确声明边界。
- 根布局一次生成 `FrameBudget` / `ContentBudget`；页面消费 typed slot，不重新读取外层窗口像素，
  不在每个页面复制 breakpoint 和 shell chrome 计算。
- 每个区域先标记为 mandatory、elastic 或 optional。mandatory 有最小可读尺寸；optional 只能
  整组显示、摘要或隐藏，不能显示半组。
- 底部安全带属于预算公式，不是渲染后的补救：最后一个 row/card 必须在 viewport 内留下明确
  的安全距离，禁止依赖 `overflow_hidden` 把错误藏起来。
- 右侧 detail/stat rail 使用独立的 label/value 槽。复合事实拆行；长值只能在自己的 bounded
  槽内换行或截断，不能让 intrinsic text 挤出标签、边框或窗口。
- 可见数据先经过 typed availability 和来源优先级；某个字段缺失不能错误地把同义可用来源
  判成空，也不能用零值伪造可用性。

## 设计顺序

1. 从根预算扣除真实 shell chrome、page padding 和固定操作行，得到页面内容槽。
2. 列出每个 slot 的完整 footprint：标题、caption、图表 floor、row、gap、padding、底部安全带
   都要计入；不能只估算“图表本身”的高度。
3. 先满足 mandatory 和 lower-band floor，再把剩余空间连续分配给 primary/elastic 区域：

   `remaining = content - fixed_chrome - mandatory_footprints - gaps - safety`

   同一布局状态内使用 `clamp(floor, remaining, ceiling)` 平滑伸缩；只有剩余空间低于 floor
   时才切换到下一个 typed degradation rung。
4. 对 lower band 做整组准入检查。准入失败时先降级次要组，不能让 flex shrink 把关键 row
   压成半行，也不能新建页面滚动来掩盖预算不足。
5. 共享几何只在 composition root / component 层实现一次。页面只声明内容和语义，不自行拼
   第二套 label/value、chart tier、scroll 或 bottom-inset 规则。
6. 用代表性真实数据检查长标题、长序列号、复合内存值、多引擎和缺失权限；数据异常不能改变
   布局 owner，也不能绕过 bounded slot。

## 收口流程

- 先写 headless geometry/behavior 断言：slot 顺序、floor、整组准入、最后一行边界、允许的
  scroll owner 和 typed unavailable 分支。
- 至少覆盖最小窗口、正常 1280×720、参考窗口、tall、wide-short、窄宽，以及长文本/多设备
  数量；测试输入必须触发真正的边界，而不是只复读实现常量。
- 再运行当前构建的真实 capture 流程。PNG、marker、window identity、source provenance 和
  独立 validator 全部通过后，才算运行证据；编译成功、fixture 成功或单张截图存在都不算。
- 对真实截图逐页目检：信息层级、大小变化、间距、对齐、右侧截断、底部安全带、空/加载/错误
  状态。发现问题后，把原因提升为共享预算/组件合同和回归断言，不只改当前页面。

## 收口前清单

- [ ] scroll owner 和不允许滚动的区域已写清楚。
- [ ] fixed / mandatory / elastic / optional slot 已列出完整 footprint。
- [ ] primary 区域按剩余空间连续分配，降级只发生在 floor 边界。
- [ ] 最后一行和右侧 value 都有可执行的 bounds 断言。
- [ ] 长值已拆行、换行或在自己的槽内截断；没有复合字符串撑栏。
- [ ] typed data fallback、权限前后状态和真实来源均有验证。
- [ ] headless 矩阵、真实截图和独立 validator 都有当前构建证据。
