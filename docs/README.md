# TaskForest 文档中心

公开文档只描述当前仍然有效的产品、架构、边界和工程规则，不保存开发流水、内部评分、
路线图、TODO、日期回执、真实主机快照或截图。

## 三级文档结构

```text
AGENTS.md                顶层工程章程：全局使命、不变量、权威路由
  ↓ 展开（不得重定义）
docs/*.md                管总：分类总纲，每篇不超过 200 行
  ↓ 展开（不得重定义）
crates/*/README.md       crate 自述：Role / Boundary / Module map /
                         Contract and verification
```

- 上层说"是什么、为什么"，下层说"在哪、怎么做"；下层可以展开上层事实，但不能重新
  定义上层契约。
- 一项事实只有一个权威来源，其他文件只链接。
- `adr/` 记录不可逆决策；不要通读，先查索引 [../adr/README.md](../adr/README.md)。
- 全仓行话（轨、generation、同异律、ingress…）定义在 [GLOSSARY.md](GLOSSARY.md)。

## 任务路由：先查这里，再读最小集合

改什么读什么，不从 AGENTS.md 顺序通读。"必读"是动手前必须过的最小集合；
"ADR"列先看 [adr/README.md](../adr/README.md) 的一句话结论，需要取舍理由才读全文。

| 任务 | 必读 | 相关 ADR |
|---|---|---|
| 新增/修改领域事实、标量或可选语义 | [SCALAR_AVAILABILITY](SCALAR_AVAILABILITY.md)、[STATE_OWNERSHIP](STATE_OWNERSHIP.md)、[WIRE_DOMAIN_BOUNDARIES](WIRE_DOMAIN_BOUNDARIES.md)、[core README](../crates/taskmanager-core/README.md) | 015 016 011 |
| 改平台采集 / 新增数据源 | [CROSSPLATFORM_STRATEGY](CROSSPLATFORM_STRATEGY.md)、[TELEMETRY_MANIFEST](TELEMETRY_MANIFEST.md) 对应能力行、[platform-contract](../crates/taskmanager-platform-contract/README.md)、对应平台 adapter README | 006 007 009 010 011 013 043 044 |
| 改调度 / 并发 / 事件投递 | [STATE_OWNERSHIP](STATE_OWNERSHIP.md)、[platform-runtime README](../crates/taskmanager-platform-runtime/README.md) | 008 033 |
| 改历史 / 图表数据 | [STATE_OWNERSHIP](STATE_OWNERSHIP.md)、[telemetry-store](../crates/taskmanager-telemetry-store/README.md)、[history-store](../crates/taskmanager-history-store/README.md) | 014 036 |
| 改前端交互 / UI 组件 | [ARCH](ARCH.md) §8、[UI_COMPONENT_ARCHITECTURE](UI_COMPONENT_ARCHITECTURE.md)、shell / ui / ui-contract / 对应前端 README | 017 020 026 027 028 038 039 042 046 051 |
| 改弹性布局 / 密集详情 / 响应式预算 | [ELASTIC_LAYOUT_PLAYBOOK](ELASTIC_LAYOUT_PLAYBOOK.md)、[UI_COMPONENT_ARCHITECTURE](UI_COMPONENT_ARCHITECTURE.md)、[QUALITY_GATES](QUALITY_GATES.md) | 038 039 |
| 动权限 / unsafe / helper | [PERMISSION_MODEL](PERMISSION_MODEL.md)、[escalation](../crates/taskmanager-escalation/README.md) 与对应 helper README | 018 019 022 023 024 025 031 035 048 049 |
| 改宿主组合 / 窗口 / 托盘 | [HOST_ARCHITECTURE](HOST_ARCHITECTURE.md)、[STATE_OWNERSHIP](STATE_OWNERSHIP.md)、[app-host](../crates/taskmanager-app-host/README.md)、[platform-native](../crates/taskmanager-platform-native/README.md) | 029 032 037 040 051 |
| 改配置 / saved-view / 启动流程 | [STATE_OWNERSHIP](STATE_OWNERSHIP.md)、[application README](../crates/taskmanager-application/README.md) | 036 040 |
| 改打包 / 安装 / 发布 | [RELEASE](RELEASE.md)、[SYSTEM_INSTALL_MANIFEST](SYSTEM_INSTALL_MANIFEST.md)、[PRODUCT_IDENTITY](PRODUCT_IDENTITY.md) | 006 029 044 045 051 |
| 新增前端 / 改产品组合 | [ARCH](ARCH.md) §2/§6、[taskmanager-cli README](../crates/taskmanager-cli/README.md)、对应前端 README | 051 |
| 新增测试 / 改测试布局 | [TEST_LAYOUT](TEST_LAYOUT.md)、[STANDARDS](STANDARDS.md) §3 | — |
| 改文档 / 流程 / 门禁 | 本页、[QUALITY_GATES](QUALITY_GATES.md) | — |

任何代码改动最终都要过 [QUALITY_GATES.md](QUALITY_GATES.md) 的本地门禁；文档与公开
边界的机械检查见其 §3、§4。

## 当前权威文档

| 分类 | 文档 |
|---|---|
| 架构、分层和数据方向 | [ARCH.md](ARCH.md)、[HOST_ARCHITECTURE.md](HOST_ARCHITECTURE.md) |
| 状态、生命周期和刷新时序 | [STATE_OWNERSHIP.md](STATE_OWNERSHIP.md) |
| Rust、模块和测试标准 | [STANDARDS.md](STANDARDS.md)、[TEST_LAYOUT.md](TEST_LAYOUT.md) |
| CI、测试和发布门禁 | [QUALITY_GATES.md](QUALITY_GATES.md) |
| 发布物和打包格式 | [RELEASE.md](RELEASE.md) |
| 贡献、安全上报和发布变更记录 | [../CONTRIBUTING.md](../CONTRIBUTING.md)、[../SECURITY.md](../SECURITY.md)、[../CHANGELOG.md](../CHANGELOG.md) |
| 权限、helper 和 unsafe 边界 | [PERMISSION_MODEL.md](PERMISSION_MODEL.md) |
| 平台采集策略 | [CROSSPLATFORM_STRATEGY.md](CROSSPLATFORM_STRATEGY.md)、[TELEMETRY_MANIFEST.md](TELEMETRY_MANIFEST.md) |
| 可用性和失败语义 | [SCALAR_AVAILABILITY.md](SCALAR_AVAILABILITY.md) |
| Wire 与领域事实边界 | [WIRE_DOMAIN_BOUNDARIES.md](WIRE_DOMAIN_BOUNDARIES.md) |
| UI 组件 | [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md) |
| 弹性布局流程 | [ELASTIC_LAYOUT_PLAYBOOK.md](ELASTIC_LAYOUT_PLAYBOOK.md) |
| 前端章程 | [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md) |
| 术语表 | [GLOSSARY.md](GLOSSARY.md) |
| 产品和桌面身份 | [PRODUCT_IDENTITY.md](PRODUCT_IDENTITY.md) |
| 安装文件边界 | [SYSTEM_INSTALL_MANIFEST.md](SYSTEM_INSTALL_MANIFEST.md) |
| 公开截图政策 | [screenshots/README.md](screenshots/README.md) |
| 第三方软件与许可 | [ACKNOWLEDGMENTS.md](ACKNOWLEDGMENTS.md) |
| 不可逆决策（索引） | [../adr/README.md](../adr/README.md) |

## 公开与私有边界

以下内容不得进入公开 Git 历史：

- 历史版本文档、变更流水和被替代方案；面向用户的版本化发布记录是唯一例外，写入
  根目录 [CHANGELOG.md](../CHANGELOG.md)；
- TODO、路线图、功能评分、差距账本和内部审计；
- 用户名、邮箱、主机名、SSID、boot ID、私有地址和绝对用户路径；
- 真实设备截图、系统快照、进程清单和安装回执；
- 凭据、签名证书、token 和密钥。

需要保留的内部材料放在本地 `.private/`；该目录被 Git 忽略。公开截图只能来自确定性
演示数据，并通过隐私检查后单独提交。

## 阅读顺序

1. 从根 [README.md](../README.md) 了解产品定位和公开成熟度。
2. 接到任务先查上方"任务路由"表，读对应最小集合；不确定时从
   [ARCH.md](ARCH.md) 出发顺依赖找。
3. 改 crate 前读 [crates/README.md](../crates/README.md) 索引与受影响 crate 的
   README（含 Module map）。
4. 需要理解设计取舍时查 [adr/README.md](../adr/README.md) 索引定位 ADR；不要从
   提交历史或内部材料反推当前契约。

## 写作规则

- 一项事实只有一个权威来源，其他文件只链接；术语进 [GLOSSARY.md](GLOSSARY.md)，
  不在各文档重复定义。
- 规则变化直接重写当前文档，不追加"本轮""增补""历史回顾"。
- 不在公开文档中记录测试数量、完成百分比、设备型号、临时路径或 commit 流水。
- "能编译""fixture 通过"和"截图存在"都不等于目标平台或发布完成。
- 新增 `docs/*.md` 必须从本页路由，否则成为孤儿文档；crate README 必须保留
  Role / Boundary / Contract and verification 三段并维护 Module map 与真实模块树一致。
- 所有本地链接、文档行数和公开边界由 CI 机械检查（`scripts/quality/doc_governance_guard.py`）。
