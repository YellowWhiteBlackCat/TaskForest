# TaskForest 当前架构

本文定义公开、当前的系统边界。实现细节归各 crate README，不在这里记录历史过程、
完成比例或主机回执。

## 1. 系统目标

TaskForest 是 Linux、Windows、macOS 三平台系统监视器。平台能力不同，但共享相同的
领域语义：数据存在、真实零、缺失、暂时失败、权限不足和不支持必须可区分。

四个前端消费同一应用投影：GPUI、Iced、Ratatui 和 Bevy。GPUI 是当前发布形态；其他
前端用于受支持的源码构建或实验性开发，不拥有独立业务事实。

## 2. 单向依赖与数据流

```text
frontend
   │ commands / render intent
   ▼
application ───────► core / shell
   │ ports              │ typed facts / pure rules / projections
   ▼                    │
platform runtime ◄──────┘
   │ scheduling / bounded collection
   ▼
app-host + native composition
   │ selected adapters
   ▼
operating system
```

返回方向只携带类型化事件、快照和失败：

```text
OS → platform adapter → runtime → application reducer → cached projection → frontend
```

前端不能直接读取 `/proc`、注册表、系统 API 或命令输出；平台 crate 不能拥有 UI 状态；
`core` 不能执行 I/O。

## 3. 层级职责

| 层 | 唯一职责 | 禁止事项 |
|---|---|---|
| `taskmanager-core` | 领域事实、可用性、纯规则、稳定身份 | OS I/O、线程、UI toolkit 类型 |
| `taskmanager-application` | 命令、reducers、ports、任务状态 | 平台 API、渲染 |
| `taskmanager-shell` | 前端中立投影、缓存和交互词汇 | 直接采集、toolkit widget |
| `taskmanager-platform-*` | 平台数据源和控制适配器 | 重定义领域语义、制造默认成功 |
| `taskmanager-platform-runtime` | 调度、并发、背压和事件传递 | UI 生命周期所有权 |
| `taskmanager-app-host` | 选择适配器并组合应用运行时 | 新建第二套事实模型 |
| frontend crates | 渲染投影并提交命令 | OS I/O、阻塞采集、业务规则分叉 |
| boundary/helper crates | 最小化原生或特权边界 | 把裸句柄、指针或未验证输入传入业务层 |

## 4. 事实与可用性

一个事实只有一个权威来源。平台层可以为上层事实增加来源证据，但不能用平台默认值覆盖
领域含义。

标量和集合必须表达至少以下状态：

- 当前有效；
- 上次有效但已陈旧；
- 暂时不可用；
- 权限不足或需要显式授权；
- 当前平台不支持；
- 已确认的真实零或空集合。

设备和进程使用稳定身份与 generation 隔离。PID、设备索引或接口名重用后，不得继承旧
对象的历史、速率基线或控制结果。

## 5. 调度与 UI 线程

阻塞采集和控制操作由 runtime 执行。application reducer 串行提交状态，shell 生成不可变
投影，前端每帧只做有界的非阻塞排水。

- 同一数据源只有一个采集所有者；
- 慢消费者不能无限扩大队列；
- 刷新请求可以合并，但控制命令和失败不能静默丢失；
- 渲染与键盘路径读取同一缓存代次；
- 窗口重建不重建平台 runtime。

详细所有权见 [STATE_OWNERSHIP.md](STATE_OWNERSHIP.md)。

## 6. 平台与组合

平台选择只发生在组合边界。业务 crate 不以 `cfg(target_os)` 建立第二套产品模型。

- Linux adapter 负责 procfs、sysfs、桌面服务和可选 helper；
- Windows adapter 通过经审计的原生边界 crate 使用 Windows API；
- macOS adapter 使用安全系统 API 和类型化降级；
- portable adapter 只提供真正跨平台且语义一致的数据。

硬件供应商是运行时能力，不是编译期产品 SKU。默认发布物包含完整的运行时 provider
注册表，不按硬件供应商拆包。

## 7. 权限与原生边界

主程序默认无特权。需要权限的操作采用固定能力、固定协议、固定可执行文件的 helper，
并把授权拒绝、helper 缺失和协议错误返回为类型化结果。

业务 crate 使用 safe Rust。`unsafe` 仅存在于四个审计边界 crate，且只能向外返回拥有
所有权、经过验证的 Rust 值。详见 [PERMISSION_MODEL.md](PERMISSION_MODEL.md)。

## 8. 前端契约

前端共享页面和命令词汇、可用性与失败语义、主题与布局 token、无障碍语义、稳定行身份、
危险操作确认，以及同一刷新代次的缓存投影。

toolkit 特有的窗口、widget、scene 或事件类型必须停留在对应 frontend crate。共享层不添加
GPUI、Iced、Ratatui 或 Bevy 类型。

## 9. 安装与发布边界

Linux DEB、RPM 和系统包从同一 staged 安装树构造；Windows MSI 的文件清单由 WiX 定义。所有
系统路径必须先进入
[SYSTEM_INSTALL_MANIFEST.md](SYSTEM_INSTALL_MANIFEST.md) 的机器可读清单。

正式版本 tag 发布必须同时成功生成 amd64/arm64 DEB、x86_64/aarch64 RPM、x64/arm64 MSI；
任何必需架构失败都使发布失败。详见 [RELEASE.md](RELEASE.md)。

## 10. 验证原则

最低成本的长期回归测试应靠近事实所有者。CI 检查格式、lint、依赖、行为、文档、安装树、
公开内容边界和发布构造；目标平台或真实硬件结论只能由对应环境验证。

真实主机截图、系统快照、日期回执、内部评分和 TODO 不属于公开架构证据，也不得进入
公开 Git 历史。质量规则见 [QUALITY_GATES.md](QUALITY_GATES.md)。
