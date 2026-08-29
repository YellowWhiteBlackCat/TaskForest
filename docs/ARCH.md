# TaskForest 当前架构

本文定义公开、当前的系统边界。实现细节归各 crate README；窗口宿主与 standalone/
layer-shell 并存合同见 [HOST_ARCHITECTURE.md](HOST_ARCHITECTURE.md)。这里不记录历史
过程、完成比例或主机回执。

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

跨 crate 不建立 forwarding facade：领域事实从 `taskmanager-core` 的 owner module 导入，
能力、请求、事件和 port 类型从 `taskmanager-platform-contract` 导入，application 只暴露
application-owned command/reducer/projection。`taskmanager-app-host` 与
`taskmanager-platform-native` 只承担组合和 adapter 选择；它们不复制或转发这些类型。

## 3. 层级职责

| 层 | 唯一职责 | 禁止事项 |
|---|---|---|
| `taskmanager-core` | 领域事实、可用性、纯规则、稳定身份 | OS I/O、线程、UI toolkit 类型 |
| `taskmanager-application` | 命令、reducers、ports、任务状态 | 平台 API、渲染 |
| `taskmanager-shell` | 前端中立投影、缓存和交互词汇 | 直接采集、toolkit widget |
| `taskmanager-platform-*` | 平台数据源和控制适配器 | 重定义领域语义、制造默认成功 |
| `taskmanager-platform-runtime` | 调度、并发、背压和事件传递 | UI 生命周期所有权 |
| `taskmanager-app-host` | 选择适配器并组合应用运行时，发布工具包无关的 surface role 合同 | 新建第二套事实模型、持有 toolkit/native surface |
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

## 8. 前端契约：同异律

### 8.1 同律

前端共享页面和命令词汇、可用性与失败语义、主题与布局 token、无障碍语义、稳定行身份、
危险操作确认，以及同一刷新代次的缓存投影。跨前端的交互语义同样属于"同"，由以下定律
约束：

1. **语义完备律**：每个用户可理解概念（优先级、挂起/恢复、结束树、服务控制、启动项、
   会话…）必须在 core 拥有中性 typed 枚举/结构体；平台原生概念（nice 值、Windows
   priority class、POSIX 信号编号、SCM/systemd 动词）是 adapter 的映射输入，不允许直达
   UI。守门：`tests/logic` 结构扫描 UI 壳层。
2. **映射穷尽律**：每个平台 adapter 对每个中性语义恰好二选一——实现映射，或 typed
   `Unsupported`。降维映射允许，但必须落在 adapter 内且覆盖枚举全域，不允许部分覆盖
   静默丢弃。
3. **同一律**：同一事实→显示的折叠全代码库只存在一份，归宿在 shell 折叠层；同一控制
   语义的 label 折叠同理一份。
4. **语义平价律**：同一投影、同一控制命令在四端三平台渲染与执行的语义必须相同——
   标签、缺失性、行序、行为后果；像素与交互手势允许不同，语义不同即缺陷。守门：
   `dual_track_policy_parity`、`renderer_fold_boundary`、`control_semantic_parity`。
5. **折叠律**：渲染入口只回放数据层折叠（"一次折叠，四端渲染"），渲染模块不得重算
   数据折叠。

### 8.2 存异边界

模型与投影永不携带 toolkit 类型、颜色、布局或平台原生词汇；刻意保留的表面差异——
精度、密度、皮肤、布局预算的执行方式、执行与游标模型（如"选中即游标"与独立 visual
cursor 可并存）、未进入共享命令表的局部按键——属于渲染层设计权。求同到语义为止，
不到像素。每个"异"必须挂在至少一个"同"上，实现同一个共享语义；没有对应"同"的
新行为不是"异"，是分叉，不得落地。

### 8.3 防串扰律

- 新交互语义先进共享层（application 命令表或 shell 交互词汇/投影），再写前端执行；
  存量平台词汇或内联折叠在触碰时中性化、下沉（触碰迁移律）。
- 声称跨前端 parity 的行为必须由行为测试钉住同一矩阵定义；无测试支撑的 parity 注释或
  文档视为缺陷。
- 修改共享语义必须四端同批落地，或显式登记未落端与原因。
- toolkit 特有的窗口、widget、scene 或事件类型必须停留在对应 frontend crate。共享层
  不添加 GPUI、Iced、Ratatui 或 Bevy 类型。
- 产品用户意图的 owner、生命周期、目标身份和四端表面裁决由
  `taskmanager-ui-contract` 的 CORE-04 注册表统一声明；新增意图必须让每个目标前端显式
  选择 shared/local/accepted-difference/typed-unsupported，不能只添加一条表面路径。

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
