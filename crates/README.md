# TaskForest 的 crate 结构

TaskForest 是一个 Rust workspace：它把「从操作系统采集事实 → 领域规则 → 应用编排 → 平台
I/O → 前端渲染」拆成一条单向依赖链。这样拆的目的很实际——平台差异、渲染技术、权限边界互不
污染，任何一处换实现都不牵动整条链。本页说明这条链分成哪几层、每层在解决什么问题；某个 crate
具体负责什么，看它自己的 `README.md`。

## 依赖方向

```text
frontend → application → core / shell → platform runtime → app-host / native composition → OS
```

返回方向只携带类型化的事件、快照和失败。一条事实只有一个权威来源；下层可以展开上层，不能重新
定义上层。

## 各层在解决什么

- **core / application / shell**：所有前端共享的领域事实、命令与投影。`core` 拥有类型化事实与
  纯规则，`application` 拥有命令、reducer 与端口，`shell` 拥有前端中立的投影、缓存与交互词汇。
- **contract**：`taskmanager-ui-contract` 与 `taskmanager-platform-contract` 规定前端和平台
  「必须提供什么」，本身不含实现。
- **platform**：provider SPI 与各平台适配器负责真实 I/O，runtime 负责调度、并发、背压与事件
  投递，app-host 负责挑选并组合平台实现。
- **frontend**：GPUI、Iced、TUI、Bevy 是四个独立产品 crate，共享同一套应用投影，区别只在渲染
  与交互；共享的 CLI harness 提供它们共同的启动骨架。
- **受审边界与工具**：只有少数受审 crate 允许 `unsafe` 或直接触碰内核/系统接口，能力按需授权、
  可单独撤销；`test-support` 与 fuzz workspace 只服务测试，不属于产品依赖。

## 怎么找到某个 crate

- 按任务查 [docs/README.md](../docs/README.md) 的任务路由表，它会指向受影响的 crate README。
- crate 的完整分母由 Cargo metadata 决定，不在文档里手工维护；每个 crate 的职责、边界与合同
  见它自己的 `README.md`（Role / Boundary / Module map / Contract and verification）。
