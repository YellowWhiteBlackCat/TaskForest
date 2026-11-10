# 跨产品共享 UI 包契约

通用 UI 能力由共享仓库 `libs/zhuge-ui-kit`（单 workspace）持有，产品负责业务和宿主适配。

| package | 当前职责 |
| --- | --- |
| `zhuge-ui-core` | 文本分段、字素边界、固定/可变尺寸虚拟窗口与其最宽带律（`virtual_list::max_band_rows`）、离散行分页（`row_page`：窗口轴 + 页码轴 + `row_budget`）、区间→单位区间的唯一投影（`value::interval_fraction`，退化答案由调用方声明）、标量线性混合的唯一算式（`easing::lerp`）与过渡时序（`easing`） |
| `zhuge-ui-tokens` | 工具包无关的已解析颜色值、sRGB/WCAG 数值事实与 transient status 的四语义色投影（`status::StatusKind` / `StatusColors::resolve`） |
| `zhuge-ui-kit-iced` | Iced surface、badge、富文本、虚拟列表 body |
| `zhuge-ui-kit-bevy` | Bevy BSN surface（card/stat row）、官方 headless button 基础场景，以及回收池的槽位回收律（`sync_virtual_slots` + `VirtualScrollSlot` 收据守卫） |

共享包发布于 crates.io（源码托管于 `YellowWhiteBlackCat/zhuge-ui-kit` 单 workspace），四个包独立发版。
根 Cargo.toml 统一在 `[workspace.dependencies]` 声明精确发布版本；子 crate 通过 `workspace = true` 继承。
产品提交 Cargo.lock，锁文件必须解析到 crates.io 源并带 registry checksum。
本地联调不得把 `[patch.crates-io]` 或 Git rev 锁文件提交成发布状态。

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

## 共享律法的机械门

共享包把每条律法的**唯一执行器**随 crate 发布：`zhuge-ui-kit-bevy` 的
`tools/check_bevy_ui.py`（BSN 唯一构造路线 + rem 唯一 authored 长度）与 `zhuge-ui-core` 的
`tools/check_band_arithmetic.py`（最宽带算术只有一个 owner）、
`tools/check_interval_normalization.py`（值进入 `0.0..=1.0` 只有一个投影）与
`tools/check_scalar_mix.py`（`A + (B - A) * t` 只有一个标量混合）与
`tools/check_easing_owner.py`（`smoothstep` 斜坡形状与「按进度解 cubic-Bézier 曲线参数」各只有一个 owner，
着色器原始字符串里的 WGSL 内建与只求值的曲线是不同事实，由 fixture 证明放行）。本仓库 `scripts/quality/` 保存这五份
文件的**逐字节拷贝**与各自的棘轮台账（`bevy_scene_ratchet.json`、`band_arithmetic_ratchet.json`、
`interval_normalization_ratchet.json`、`scalar_mix_ratchet.json`、`easing_owner_ratchet.json`）；
台账记录执行器的摘要与所属函数，vendored 拷贝被改动而未同步台账时，`test_*_gate.py` 立即失败，
并同时证明台账外的新增违规会咬（含同文件两处逐字相同违规各算一笔）。律法正文只有共享包 crate README 一处，产品不得另写第二套检查或第二套算术。

## 升级与验证

共享包各 crate 独立处于 `0.x`：功能与修复升 patch，破坏性更新升 minor（例如 `zhuge-ui-kit-bevy 0.3.0 → 0.4.0`）；
1.0 起按 SemVer：major 破坏、minor 兼容增加、patch 兼容修复。
公开类型、工具包版本、MSRV、feature、平台支持以及布局/焦点/滚动变化均需兼容性评估。
共享包发布要求由其仓库的 `docs/EVOLUTION.md` 持有；产品升级前阅读迁移说明。

本地联调使用共享仓库的 `scripts/local_patch.py enable/disable/status` 或本机 Cargo `[patch]`，
只修改开发机配置，不提交产品 path 覆盖。
提交前关闭 patch，使用 `--locked` 检查；构建与捕获证据必须与锁文件版本一致。
`test_release_pin_parity.py` 机械拒绝非 crates.io 源、缺失 checksum 与版本漂移。
包版本升级由各产品按需决定，不迫使所有产品同步升级。

验证要覆盖受影响构建、行为/几何测试、仓库门禁和真实界面。
截图必须绑定实际二进制与源码；Cargo 声明的共享包源码清单属于该源码绑定，
其外部路径必须仍落在当前前端的生产依赖图中，并逐文件通过摘要校验。没有目标平台证据就记录未验证，不能以编译通过代替真实运行实证。

本产品完成前运行 `bash scripts/quality/local-gates.sh quick`；前端捕获和布局规则
由 [UI 组件章程](UI_COMPONENT_ARCHITECTURE.md) 与受影响 crate 的文档展开。
