# TaskForest 文档中心

公开文档只描述当前仍然有效的产品、架构、边界和工程规则，不保存开发流水、内部评分、
路线图、TODO、日期回执、真实主机快照或截图。

## 文档分层

```text
README.md                  产品介绍与公开状态
  ↓
AGENTS.md                  全局工程章程与不变量
  ↓
docs/*.md                  当前分类总纲（每篇不超过 200 行）
  ↓
crates/*/README.md         crate 职责、契约和验证方式
```

`adr/` 记录当前不可逆决策。下层文档可以展开上层事实，但不能重新定义上层契约。

## 公开与私有边界

以下内容不得进入公开 Git 历史：

- 历史版本文档、变更流水和被替代方案；
- TODO、路线图、功能评分、差距账本和内部审计；
- 用户名、邮箱、主机名、SSID、boot ID、私有地址和绝对用户路径；
- 真实设备截图、系统快照、进程清单和安装回执；
- 凭据、签名证书、token 和密钥。

需要保留的内部材料放在本地 `.private/`；该目录被 Git 忽略。公开截图只能来自确定性
演示数据，并通过隐私检查后单独提交。

## 当前权威文档

| 分类 | 文档 |
|---|---|
| 架构、分层和数据方向 | [ARCH.md](ARCH.md) |
| 状态、生命周期和刷新时序 | [STATE_OWNERSHIP.md](STATE_OWNERSHIP.md) |
| Rust、模块和测试标准 | [STANDARDS.md](STANDARDS.md)、[TEST_LAYOUT.md](TEST_LAYOUT.md) |
| CI、测试和发布门禁 | [QUALITY_GATES.md](QUALITY_GATES.md) |
| 发布物和打包格式 | [RELEASE.md](RELEASE.md) |
| 权限、helper 和 unsafe 边界 | [PERMISSION_MODEL.md](PERMISSION_MODEL.md) |
| 平台采集策略 | [CROSSPLATFORM_STRATEGY.md](CROSSPLATFORM_STRATEGY.md)、[TELEMETRY_MANIFEST.md](TELEMETRY_MANIFEST.md) |
| 可用性和失败语义 | [SCALAR_AVAILABILITY.md](SCALAR_AVAILABILITY.md) |
| Wire 与领域事实边界 | [WIRE_DOMAIN_BOUNDARIES.md](WIRE_DOMAIN_BOUNDARIES.md) |
| UI 组件 | [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md) |
| 产品和桌面身份 | [PRODUCT_IDENTITY.md](PRODUCT_IDENTITY.md) |
| 安装文件边界 | [SYSTEM_INSTALL_MANIFEST.md](SYSTEM_INSTALL_MANIFEST.md) |
| 公开截图政策 | [screenshots/README.md](screenshots/README.md) |
| 第三方软件与许可 | [ACKNOWLEDGMENTS.md](ACKNOWLEDGMENTS.md) |
| 不可逆决策 | [../adr/](../adr/) |

## 阅读顺序

1. 从根 [README.md](../README.md) 了解产品定位和公开成熟度。
2. 工程改动先读 [AGENTS.md](../AGENTS.md) 和本页对应分类总纲。
3. 再读 [crates/README.md](../crates/README.md) 及受影响 crate 的 README。
4. 需要理解设计取舍时查对应 ADR；不要从提交历史或内部材料反推当前契约。

## 写作规则

- 一项事实只有一个权威来源，其他文件只链接。
- 规则变化直接重写当前文档，不追加“本轮”“增补”“历史回顾”。
- 不在公开文档中记录测试数量、完成百分比、设备型号、临时路径或 commit 流水。
- “能编译”“fixture 通过”和“截图存在”都不等于目标平台或发布完成。
- 所有本地链接、文档行数和公开边界由 CI 机械检查。
