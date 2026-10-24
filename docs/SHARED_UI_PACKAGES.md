# 跨产品共享 UI 包契约

通用 UI 能力由共享仓库 `libs/zhuge-ui-kit`（单 workspace）持有，产品负责业务和宿主适配。

| package | 当前职责 |
| --- | --- |
| `zhuge-ui-core` | 文本分段、字素边界与固定/可变尺寸虚拟窗口 |
| `zhuge-ui-tokens` | 工具包无关的已解析颜色值 |
| `zhuge-ui-kit-iced` | Iced surface、badge、富文本、虚拟列表 body |
| `zhuge-ui-kit-bevy` | Bevy BSN surface、官方 headless button 基础场景 |

共享包发布于 crates.io（源码托管于 `YellowWhiteBlackCat/zhuge-ui-kit` 单 workspace），四个包独立发版。
根 Cargo.toml 统一在 `[workspace.dependencies]` 声明版本（如 `zhuge-ui-core = "0.1.3"`）；子 crate 通过 `workspace = true` 继承。
产品各自提交 Cargo.lock，保留独立依赖锁定与发布周期。

## 产品开发流程

1. 新增组件或修改通用行为前，先查共享包 crate README 的模块 owner。
2. 如果现有能力不足，优先扩展对应共享包，然后接入产品。禁止在产品复制同类算法、
   创建转发 facade 或以本地同义 API 隐藏共享 owner；直接 import 定义模块。
3. 先判定差异是否必要。业务、平台或无障碍确有不同才保留；纯历史习惯应统一。
   必要差异通过显式布局/颜色/搜索策略参数或产品适配表达，不按产品名分叉。
4. 产品继续拥有业务命令、筛选谓词、拼音匹配、文案、主题预设、宿主与平台 I/O。
   Iced 与 Bevy 的状态分别留在自己的工具包，不强制抽象两者没有交集的能力。
5. 改共享包同时检查 TaskForest 与 MusicFrog 的受影响消费者；不要只验证当前产品。
   同一切片迁完旧调用并删除被替换实现，保留既有业务、布局、输入和无障碍契约。

## 升级与验证

共享包当前是 `0.1.x`：同一兼容线内的功能/修复只升 patch；破坏性更新升到 `0.2`。
稳定版本按 SemVer：major 破坏、minor 兼容增加、patch 兼容修复。
公开类型、工具包版本、MSRV、feature、平台支持以及布局/焦点/滚动变化均需兼容性评估。
共享包发布要求由其仓库的 `docs/EVOLUTION.md` 持有；产品升级前阅读迁移说明。

本地联调使用共享仓库的 `scripts/local_patch.py enable/disable/status` 或本地 Cargo `[patch.crates-io]`，
只修改本机开发配置，不提交产品 path 覆盖。
提交前关闭 patch，使用 `--locked` 检查；构建与捕获证据必须与锁文件版本一致。
包版本升级由各产品按需决定，不迫使所有产品同步升级。

验证要覆盖受影响构建、行为/几何测试、仓库门禁和真实界面。
截图必须绑定实际二进制与源码；没有目标平台证据就记录未验证，不能以编译通过代替真实运行实证。

本产品完成前运行 `bash scripts/quality/local-gates.sh quick`；前端捕获和布局规则
由 [UI 组件章程](UI_COMPONENT_ARCHITECTURE.md) 与受影响 crate 的文档展开。
