# TaskForest 质量门禁

本文定义当前公开仓库的验证层级。门禁证明行为、边界和发布构造，不使用内部评分、截图数量或主机回执替代产品验证。

## 1. 合并门

对 `main` 的 pull request 和 push 同时运行 Linux 与 Windows 阻塞 CI：

| 检查 | 目的 |
|---|---|
| public-repo guard | 拒绝私有路径、真实截图、个人邮箱、凭据模式和主机路径 |
| cargo-deny | 拒绝已知漏洞、不允许的许可证和依赖策略违规 |
| Python/Shell 政策门 | 检查安装清单、自动化安全、模块边界和测试布局 |
| rustfmt / clippy / production-config | 格式、warning-free 编译与不带 test-support 的生产配置 |
| Bevy 光学能力声明门 | 把 `COMPLETE` 声明绑定到实时 `RenderAdapterInfo`、生产 WESL 证明与新鲜 receipt，禁止 palette 常量冒充证据 |
| 公共资源单源门（ADR-063） | standard `nextest-logic` 与 CI `test` job 阻塞：文案侧 `only_the_owner_holds_the_message_layer` 零容忍第二字典/`Language`/语言字面表，`no_frontend_paints_chinese_copy_the_catalog_does_not_own` 零容忍未入库中文，`t` 字面量/动态键/键对称门零容忍悬空 key，`hard_copied_widget_copy_stays_within_its_audited_census` 五端全 0；主题与字型侧（`typography_authority_gate_test`）`no_frontend_spells_a_font_family_name`/`no_frontend_spells_a_hardcoded_type_size`/`no_frontend_spells_a_hardcoded_font_weight`/`no_frontend_reimplements_a_theme_axis_codec` 四门零容忍，`the_bundled_font_collection_is_the_asset_table` 与 `the_public_axis_codecs_round_trip_and_match_the_wire_tokens` 焊死字节表与 token 词汇；每个扫描门都配阴性对照自证非空，新增绘制/字号入口须同变更登记 |
| release build | 验证四端独立前端产品发布形态（PR 用无 LTO 冒烟 profile，main 与 tag 用完整发布 profile） |
| package stage simulation | 验证 Linux 安装树、权限和 polkit 路径 |
| workspace nextest | 行为和平台无关契约 |
| public API semver gate | 拒绝相对基线的 Rust 签名级破坏（基线是 git rev） |
| doctests / rustdoc | 文档代码和公开 API 链接 |
| fallback feature matrix | 验证可选 provider 不产生产品 SKU 分叉 |
| CORE-04 functional matrix | 验证每个产品意图在 GPUI/Iced/TUI/Bevy 都有显式 surface decision |
| capability delivery registry | 拒绝参考层把语义合同能力声明为已挂载组件（`Reference`）；语义合同须声明 `Ported`，集合只按挂载证据增长 |

CI 与本地门禁都使用 [`rust-toolchain.toml`](../rust-toolchain.toml) 声明的 1.99.1；`Cargo.toml` 的 `rust-version` 仅是兼容性下限。
所有 Cargo 验证使用锁文件，并行度不超过四。Windows 原生边界以及 macOS 编译/库测试在每次 PR 与 main push 的
portability workflow 中阻塞运行；macOS 打包和真实设备视觉验证仍然 deferred，跨平台编译不能替代原生 API 或设备证据。

## 2. 本地层级

### 强制测试执行器规则

这是不可豁免的仓库规则：除 doctest 外，任何测试、集成测试、GUI/headless 测试和
覆盖率测试都必须通过 `cargo nextest run ... -j 4`（覆盖率使用
`cargo llvm-cov nextest ... -j 4`）。测试命令只允许在明确带有 `--doc` 的
doctest，并且必须带 `-j 4`；裸测试命令、带 `--tests` 的变体或其它非 doctest
形式都会被 `scripts/quality/test_runner_guard.py` 拒绝。`cargo nextest list` 仅用于发现，
不能替代实际的 `nextest run`。

统一入口为：

```bash
bash scripts/quality/local-gates.sh quick
bash scripts/quality/local-gates.sh standard
bash scripts/quality/local-gates.sh extended
```

- `quick`：公开边界、文档、格式、依赖版本底线、模块、安装清单、自动化、测试执行器和测试布局政策门，`shared-kit-git-pin`/`release-pin-parity`（两产品必须锁定共享私有 kit 的同一不可变 Git `rev`，锁文件不得带本机 `[patch]` 的 path/registry 源），以及 CI/本地 clippy 命令口径守卫 `clippy-parity` 和 production-config 接线守卫 `production-config-wiring`（断言本地 standard stage、CI `lint` job 与 portability macOS job 三处仍调用 helper），以及结构棘轮阈值上限守卫 `clippy-ratchet`（断言 `clippy.toml` 的复杂度/行数阈值不超过已审计上限，抬高阈值必须同变更改守卫），以及裸 `allow` 上限守卫 `allow-ceiling`（`dead_code` 一律禁止，其余 lint 种类按已审计计数只降不升，新增种类必须同变更登记），以及公共 API 兼容门禁 `semver-checks`（相对基线的 Rust 签名级破坏即失败；其 scratch 位置由 `semver_scratch.sh` 单一 owner 决定：`SEMVER_SCRATCH` → 仓库 `.tmp/semver-gate` → `TMPDIR`，每个候选都要经真实写入探针，**仓库自有 scratch 必须压过环境 TMPDIR**——否则配额耗尽的 /tmp 会让这道 blocking 门以 exit 2 悄悄变成 SKIP，比假 FAIL 更危险；quick 阶 `semver-scratch-self` 八格证明顺序/回退/探针/拒绝/去重/未设 TMPDIR/`df`），以及 owner 路径守卫 `inline-path`（owner 类型必须用 `use` 在模块边界导入，禁止内联全名，也禁止 `as` 别名——后者由 `rust-surface-guard` 管），以及 Flatpak 清单门 `flatpak-manifest` 与 Linux 沙箱分发面门 `linux-sandbox-packaging`（Flatpak 清单/AppStream 组件 id/安装路径与 AppImage 发行名 + 无 linuxdeploy 的 AppDir 无头自检），与长效浸泡防泄漏门 `soak-stability`（宿主 RSS/FD 探针：**两类序列都按形状判定**。FD：settled 区内继续创新高、或不归还的慢阶梯才算泄漏；一次阶跃后已有 ≥10 个样本停在最大值是平台（实测 Bevy demo 27⇒31 后 154 个样本恒平，旧判据 `final > initial + 3` 会把这个形状在整窗判红）；平坦但触及 4096 仍按结构性上界判红；涨幅超容差而平坦证据不足时显式判 `unresolved`。RSS：25% 首末增长硬阈值不变，但只在末端稳态窗口（≥10 个样本且 ≥settled/3）无新高、波动 ≤12%、线性漂移 ≤25%、且峰值不超过已观测工作集 1% 时，才把启动后延迟扩张判为一次性平台；持续创新高或尾段继续漂移仍是泄漏，短样本和证据不足绝不假绿；序列按 (target, pid) 分生命周期，重启前后不得拼成一条增长；浸泡 runner 不再自己重写 RSS/FD 判决，实跑与 `--report-only` 两条路径共用这一个 owner，二进制另存私有快照后再不随 `target/debug` 的重建而失真）和根浸泡套件 `tests/soak.rs`（100ms 节奏驱动真实采集循环，断言历史环按容量淘汰、描述符/RSS 不漂移、资源物化不累积；RSS 与 shipped owner 共用同一稳态形状，另有 Arm D 对每个 settled tick 断言**跨进程 /proc 句柄的绝对上界**与 `Max open files` 软限额不被采集移动——first-vs-last 漂移看不见「每进程常驻一个句柄」这种第一 tick 即达顶的结构性成本，所以绝对界与漂移界必须并列；`TM_SOAK_TICKS`/`TM_SOAK_TICK_MS` 可把默认 60 tick 拉成长驻留浸泡），以及 Bevy 场景律法门 `bevy-bsn-guard`（唯一 `bsn!` 构造路线）、`bevy-rem-guard`（rem 尺寸轴：零 `FontSize::Px`、px 派生值不作布局长度、图标适配器不收裸数字尺寸）与 `bevy-scene-gate`（共享包 `zhuge-ui-kit` 随 crate 发布的唯一执行器：禁硬编码非零 px 字面量，存量记在 `bevy_scene_ratchet.json` 棘轮台账，台账外新增即失败）、`band-arithmetic-gate`（共享包 `zhuge-ui-core` 随 crate 发布的带宽算术唯一律执行器：向上取整的行数再叠加 overscan/band，或据其命名池容量与挂载上界，除调用 `max_band_rows` 外一律失败；池下限 `.max(8)` 是策略、放行。存量记在 `band_arithmetic_ratchet.json`，台账记录执行器摘要，`band-arithmetic-gate-self` 证明 vendored 拷贝未漂移且会咬）、`interval-normalization-gate`（同包随 crate 发布的区间投影唯一律执行器：语句或同函数内把「减下界 / 除以区间长度 / 夹到 0..=1」三步自己写全，除调用 `value::interval_fraction` 外一律失败；改名后的分母（`UIK-VALUE-003`）按绑定它的差式识别，不靠命名；本地助手做商、调用方再钳单位区间同样判红（`UIK-VALUE-004`）；沿段无 clamp 的比例、像素覆盖斜坡与槽位预算助手是不同事实，由 fixture 证明放行；四门台账指纹均含所属函数，同文件两处逐字相同的违规各算一笔；存量记在 `interval_normalization_ratchet.json`，`interval-normalization-gate-self` 证明拷贝未漂移且会咬）、`scalar-mix-gate`（同包随 crate 发布的标量混合唯一律执行器：`A + (B - A) * t` 及其交换写法除调用 `easing::lerp` 外一律失败；进度的钳制与结果域钳制属调用方策略、放行；逐分量色/向量混合是类型自己的混合面、未声明共享依赖的 crate 无 owner 可调，两类均由 fixture 证明放行；存量记在 `scalar_mix_ratchet.json`，`scalar-mix-gate-self` 证明拷贝未漂移且会咬）、`easing-owner-gate`（同包随 crate 发布的缓动归属唯一律执行器：手写 `t * t * (3.0 - 2.0 * t)` 斜坡或本地再定义 `smoothstep`，除调用 `easing::smoothstep` 外一律失败（改名或上提平方不豁免）；同函数内既迭代收窄参数又求值三次多项式的 Bézier 缓动解算同理失败，而只求值曲线画线段、非 Bézier 的 Newton/二分是不同事实；着色器源码里的 WGSL 内建 `smoothstep` 整段掩掉不视为 Rust；未声明共享依赖的 crate 无 owner 可调，由 fixture 证明放行；存量记在 `easing_owner_ratchet.json`，`easing-owner-gate-self` 证明拷贝未漂移且会咬）；`semantic-tree-owner` 禁止四端再写共同状态行/CPU 图/64 行上限/进程数值，四 shape 结构签名由 `taskmanager-shell::accessibility_projection` 逐位对齐；`i18n-vocabulary-owner` 禁止前端 crate 自造 typed vocabulary 的目录键或自行把 `t(<x>.i18n_key())` 组合成本地文案——告警指标、严重级别、服务动作、会话动作、NPU 引擎、开机档位与范围、溢出计数等词汇一律调用 `taskmanager-application::i18n` 的具名解析器，词表键由 `taskforest-core` 拥有；唯一的例外是 GPUI 窗口装饰裁决这一单消费者词汇，其理由与触发条件登记在守卫内，第二个前端渲染同一事实即须迁移；`percent-readout-owner` 禁止任何非 owner 层自造百分比拼写：格式串同时带 `%` 与占位符即判红，前端与导出、无障碍快照、桌面通知、应用层都必须调用 `taskforest_core::core::percent` 的拼写（水平值整数百分比、速率一位小数）与 `taskmanager_shell::presentation::percent` 的缺省感知读数；唯一例外是历史 scope id 的 URL 转义 `%{byte:02X}`（非百分比单位），其理由与触发条件登记在守卫内。同一执行器还禁**站内 percent 域钳制**：`clamp(0.0, 100.0)`（含 `_f32`/`_f64` 后缀、`clamp(0.0, MAX_PERCENT)` 半绑形态与 `.max(0.).min(100.)` 链式反序）必须调用 `taskforest_core::core::percent::ClampedPercent`；注释与字符串里的旧拼写不参与；唯一例外是零 workspace 依赖的 Windows ABI 信任根 `taskmanager-windows-api`（ADR-031，无通往 owner 的边），其理由、**计数**与触发条件登记在守卫内，多一处即红；`failure-severity-owner` 禁止任何非 owner 处再回答「部分失败源由哪一条 failure 代表」：`FailureKind` 的秩表只允许在 `taskforest-core::core::failure` 一处出现（秩 crate 私有，外部比较直接编译失败），任何 `severity_rank` 读数即判红——折叠一律走 `FailureKind::stronger`/`FailureKind::strongest`（对 `None` 取候选、平秩保先观测者）；owner 处 9 个变体必须恰各一次、秩为严格全序 1..9；R135 前 24 个子系统分属四 crate 各写各表、共九种互斥排序（17 处与采纳表不一致，9 处把 `IdentityChanged` 与 `Rejected` 并列、合并结果取决于实参顺序），全部收敛于此；`ProviderFailure`/`SmartProviderFailureKind`/`NvmlFailureKind`/`ProcessMetadataFailure` 是各自词汇的排序事实，不判红；`threshold-band-mirror` 禁止**只被部分 shape 花费**的 band 在共享 vocabulary 里再写一份：GPUI/Iced 的窗口有 720x480 下限、终端自己拥有视口、Bevy 取合成器给的大小，这种「数值共享但花费与否不共享」的 band 只有一个合法地址——threshold registry（只有它的 `Own`/`Absent` 机制能说清谁在花）；registry 的 `is_optional` 轴基线不得作为 `vocabulary` 的 `pub const` 再现（按值判定，故改名也逃不掉；每个 shape 都花的轴不在此列，那是不同事实）；R136 实测发现 vocabulary 里那份 720/480 无人读取、而 registry 自己那行是死的，GPUI 另有一份镜像常量、Bevy 声明谎称 `Shared`；`alert-marker-owner` 禁止 owner 之外再写告警标记这个码点（裸字符、`\u{26a0}`、`\u26a0` 三种拼法一律判红，注释不判以免门被杂文逼成摆设），也禁止把它存进 `locales/*.json`（目录管词，标记进目录即同记号第二个地址），owner 内该码点必须恰一处、`presentation` 必须仍导出它，且不得把标记定义成 `icon_glyph(IconId::Alert)`——那是不可带装饰码点表面的单色后备（`△`），与文本标记是不同事实，折叠即重画所有文本面；三条穿了标记的 SMART 读数（`endurance_used_readout`/`spare_pool_readout`/`smart_temperature_label`）只允许在 `presentation::smart` 声明一次，前端只能调用；R138 实测该码点被四个前端手写九处，其中三处还各自重写同一读数——一处用了第二个百分比助手，两处把英文`warning` 写进源码而非目录，于是中文用户看到英文；`control-failure-copy-owner` 禁止 owner 之外再拼进程控制通知的模板（`feedback.process_action_failed` 与 `feedback.process_action_succeeded` 两个字面量只允许出现在 `taskmanager-shell` 的 `platform_feedback::process_control_notice_text` 一处），禁止再声明 `control_error_detail`／`process_control_notice_text`／`process_control_action_label`／`typed_failure_reason`／`process_control_feedback` 中任何一个（后两者是已退役的第二权威，任何文件都不许声明），禁止任何前端自造 `enum ProcessControlAction` 动作词汇（必须绑定中性 `ProcessControlKind`），并要求拼通知的文件按名导入 owner 的投影、owner 仍导出它；同一执行器还查封 **CPU 亲和性读结果的投影面**：五个具名亲和面（GPUI `processes_view/chrome/overlays.rs`、TUI `ui/affinity_modal.rs` 与 `ui/process_details/insights.rs`、Iced `ui/affinity.rs`、Bevy `pages/processes/affinity.rs`）必须各自绑定 `control_error_detail`，任何 `taskmanager-*` crate 的代码不得自写 `affinity unavailable` 这类英文句子（句子属目录；源码里的英文会抵达每一种语言），亲和面内也不得对 typed failure 做 `Debug` 转储（这正是 TUI 自己写明的 G-04b）；R140 实测 Iced 亲面在源码里写 `format!("Current affinity unavailable: {failure:?}")`，TUI insights 的亲和臂 `_ => {}` 让整行消失、TUI 模态在失败后永远停在「collecting」，Bevy 模态则直接编造 all-CPU 默认掩码且从不消费 `Failed`；R141 再把同一执行器延伸到 **control outcome 文案**：`FailureKind → key` 折表下沉到 `taskforest-core::core::failure::i18n_key`（application i18n 是唯一 key→语言解析，`shell::presentation` 的转发门面删除，28 个消费点按名直绑），`taskmanager-application::i18n::control_outcome_text` 是 session/startup/service 三类结果的唯一装配（成功分 accepted/completed 两种目录模板、失败一律用共享原因），禁 core 之外的 `FailureKind → feedback.*` 折表、源码里的 `"Session … request accepted"`/`"Startup … completed"`/`"Service … failed"` 英文句与结果面上的 `{failure:?}` 转储；R141 实测 shell 三条 notice 全写死英文并对 typed failure 做 `Debug` 转储、`service_log_rejection_detail` 把 `{failure:?}` 填进 `{reason}`、GPUI/TUI/Iced/Bevy 各自重拼 session 结果句（TUI/Iced 还用 `{:?}`），并顺带逼出 `gpui/root/batch_process.rs` 一份第二 key 折表（`IdentityChanged→process_gone`、两个 provider-unavailable 变体→`health.failure_provider_unavailable`，与 owner 分歧），已一并硬切删除；R139 实测 GPUI 曾同时持有两份原因表——`root/platform_lists.rs` 那份与 owner 逐字相同、`root/process_feedback.rs` 那份在九分之六的 `FailureKind` 上与之分歧（其中四处把英文 detail 写死进源码），且同一文件另带一个有三条从未被构造变体的动作词汇并重拼同一模板；`wired-tests` 禁止死测试：每个 crate `tests/` 下的 `.rs` 文件都必须被某个会被编译的文件通过 `#[path = ...]`、`mod ...;` 或 `include!(...)` 挂载（或由 Cargo.toml 的 `[[test]] path=` 显式声明为测试目标），否则判红——从未编译的测试不构成任何证据（R95 的百分比 owner 行为测试即以此形态长期未被编译，本轮补挂并加门防复发）。；`chart-slot-owner` 禁止前端再写一份「样本窗口 → 像素槽位」网格：不得声明本地 `WindowSlots`/`ChartSlots` 或本地 `denominator` 助手，不得内联复刻分母律（`saturating_sub(1).max(...).max(1)`），三端图表面必须绑定 `taskmanager_shell::presentation::ChartSlots` 这一个 owner。；`input-modality-owner` 禁止任何 crate 再造一份「输入模态 → 是否画焦点环」的词汇：除 `taskmanager-ui-contract::InputModality` 这一个 owner 外不得声明本地 `enum InputModality`，前端只能委托焦点环决策而不得自行重写（`shows_focus_ring` 的实现体不得出现 `Keyboard` 变体），不得把 owner 类型再导出成转发门面，GPUI / Iced / Bevy 三端必须绑定该 owner。其中 `scripts/quality/test_runner_guard.py` 机械拒绝非 doctest 的裸 Cargo 测试入口及缺少四并行度的测试执行。具备宿主 Wayland/KWin 依赖时还运行真实私有 A/B 隔离测试；可用 `TM_CAPTURE_ISOLATION_GATE=1` 强制运行，缺少环境时 `auto` 只记录明确的 SKIP；**运行中途才消失的前提同样记 SKIP 而非 FAIL**（冷 target 目录在 `TM_CAPTURE_PREWARM_SECONDS`（默认 300s）内编不出 capture 二进制 ⇒ `IsolationUnavailable`）：A/B 前先串行预热那次共享 capture 编译，避免两个私有会话同时冷编译、串行争用同一 build lock 而双双跑不出验证窗口（这是 R124 宿主态 FAIL 的真因，不是隔离被破坏）。stage 预算由 `CAPTURE_ISOLATION_TIMEOUT`（默认 600s）给出，独立校验器预算 `CAPTURE_VALIDATOR_TIMEOUT`（默认 180s，实测工作量 26–41s），且其超时（124/137）与「校验器确实拒绝」按退出码分流，不再把 SIGTERM 折叠成产品判决。另有五个实时结构普查私有门按能力探测接入 quick：`funnel-adjudication-self` 用实时跨产品漏斗证明每一条 exact/near/kit-covered 命中都有带理由与触发条件的族裁决（R145 实跑前 18/19，新增两条未登记 near 后 19/19），`numeric-fact-census` 扫三仓生产树并要求每条 TF/MF 同形且 kit 未拥有的浮点事实都有书面归类（R145 新声明 `percent/100.0` 后 `0` 未声明候选）；`frontend-duplicate-adjudication` 把四端重复裁决的默认源从 R89d 静态快照改为实时漏斗（R147 实时 `pass 96/0`，旧快照配新族表曾假报 38 violations），`owner-import-shape` 禁两产品可执行码再写 `zhuge_ui_*` 全限定 owner 路径（公共 `inline-path` 同步扩面，两处 `interval_fraction` 已改模块边界 `use`），`owner-family-call-sites` 按 R71 基线复核 easing/curve/pixel/status 调用点（R147 刷新 33 站点、0 violations）；五者均默认读实时源码而非静态证据快照，避免代码演进后旧回执继续冒充绿灯。另有 `background-compositor-guard` 按同一能力探测接线，以 AST 拒绝任何私有 GUI 启动器把裸 Wayland socket、宿主 display 或错误 `start_host` 元组交给产品，强制所有窗口只连嵌套虚拟 KWin/Niri 的绝对私有 socket；工具与兄弟产品树都属忽略的私有宿主材料，**clean CI clone 中两 stage 必须显式 SKIP**，不得把缺失文件报成产品失败；`TM_PRIVATE_CENSUS_GATE=1` 强制开启时缺能力则真 FAIL，`=0` 显式跳过。两个私有普查器自身还做空根 fail-closed：任一仓没有 Rust 源码即 `ValueError`，绝不把空集读成 `candidates=0` 的假绿（R146 isolated 沙箱实测：auto SKIP / forced FAIL / 宿主 full PASS）。
- `standard`：quick + dependency audit、clippy、production-config、nextest、doctest、rustdoc、release build、平台无关形态矩阵，以及 Linux release/package smoke；
- `extended`：standard + coverage、mutation、Miri、fuzz 和性能/体积回归。

**范围隔离**：并行前端线共用一个工作区时，追加 `--scope <core|bevy|gpui|iced|tui>`
把 cargo 阶段（fmt/clippy/nextest/doctests/rustdoc）和源码面政策门限制在该前端的
"core + 依赖闭包 + 自身 crate"（闭包由 `cargo tree` 从锁文件推导，不是手工清单）；
跨前端的根验收层、形态矩阵和 release smoke 记为 SKIP，由主线/合并负责人在 `all`
（默认）下承担。例：Bevy 线的日常门禁是
`bash scripts/quality/local-gates.sh standard --scope bevy`（Bevy 交互矩阵为 headless，
scoped 下随 standard 直接运行）。

**共享锁降级**：开发阶段若另一条线正持续改写共享 Cargo.lock，`lock-consistency` 探针等不到稳定窗口时，本次运行
显式降级为不带 `--locked`（summary 记 FALLBACK，子脚本经 `TM_CARGO_LOCK` 同步），而不是各阶段以晦涩的锁错误
失败；合并前合并负责人仍须以 `all` + 锁定模式重跑。

默认遇到首个失败即退出；需要一次性收集多个失败时显式追加 `--keep-going`。Linux `release/package smoke` 与 CI 使用
同一入口，平台不具备 Linux 打包能力时不在 Windows/macOS 本地门禁中伪造通过。

Windows 开发机使用 `scripts/windows/local-gates.sh`，通过 Git Bash 调用同一组可移植门禁；
Windows telemetry、测试和 helper 不使用 PowerShell 或其他命令解释器采集系统事实。

Bevy UI 资源访问由 `scripts/quality/bevy_world_access_guard.py` 在 quick 与 CI 中检查：
生产源码不得接收整世界或通过 App、hook、排队闭包绕过显式参数；范围见 [Bevy 章程](../crates/taskmanager-bevy-ui/docs/BEVY_UI_FRONTEND.md)。

### 依赖解析与 API 兼容门禁

- **公共 API 兼容门禁（semver）**：阶段名 `semver-checks`，属 standard 层（要重建当前树与基线树，放不进 quick 预算）；缺 `cargo-semver-checks` 时本地 SKIP、CI 必装并阻塞；**scratch 写不下也记 SKIP（exit 2，门不可用）**：门先在选定 scratch（`SEMVER_SCRATCH` → `TMPDIR` → 仓库忽略的 `.tmp`）上做 128 MiB 实写容量探针，配额/空间不足的 I/O 失败绝不报成「破坏了公开 API」（R125 前 `/tmp` qgroup 耗尽即产生过这种假 FAIL）。基线遵循 ADR-060：发布库用 `taskforest-*` 命名空间，**首次发布前**基线是 git rev（`--baseline-rev`），**首次发布后**改用该已发布版本；基线里不存在的包名按新 crate 记 SKIP（改名后首批发布正属此形态），但**一个都没比较就判失败**——不接受「全员 SKIP 读成绿色」。通过只表示相对基线没有 Rust 签名级破坏，不证明行为/存档/协议兼容或平台运行正确性。**未发布的 breaking 版本 bump 不得假绿**：`cargo-semver-checks 0.51.0` 见到 major bump（含 `0.5→0.6` 的 0.x 破坏步）会输出 `0 checks: 260 skip` 并判 `no semver update required`；门现在把当前 rustdoc JSON 的版本强制回基线版本，让所有 lint 真跑，只有 `N major and 0 minor/patch checks failed` 才记为 `planned major bump`（列出被删 API 并提示发布后才能成为新基线），任何 minor/patch 级破坏仍 FAIL；quick 阶 `semver-major-bump-self` 三格钉住纯 major / 纯 minor / mixed 三类摘要不会被互相放过。
  **锁文件钉住的两侧构建**：0.51.0 无 `--locked`（基线侧本由工具占位工作区解析，不受仓库锁文件钉住）；门禁默认改走 `--current-rustdoc`/`--baseline-rustdoc`：`git archive` 落地基线树，两侧各以**自己那棵树**的 `cargo rustdoc --locked` 生成 rustdoc JSON（需 nightly；`SEMVER_RUSTDOC_TOOLCHAIN`/`SEMVER_RUSTDOC_JSON` 可控），工具只比对两份 JSON，于是**两侧都受各自锁文件钉住**，"所有 Cargo 验证使用锁文件"不再有例外；该流程不可用（工具太旧/缺 nightly/python3）时显式记录并回退 `--baseline-rev`，只改变基线侧钉法，不改变结论口径。
- **依赖解析与 lint 事实源门禁**：resolver v3（ADR-057）让依赖解析尊重 `rust-version` 下限；根
  `[workspace.lints]`（ADR-056）是唯一 lint 事实源，业务 crate 继承它，四个 unsafe 信任根 crate 显式声明例外，
  测试期 unsafe 必须登记。

## 3. 公开仓库门禁

`scripts/quality/public_repo_guard.py` 检查 Git 跟踪内容：

- 禁止 `.private/`、`docs/archive/`、内部评分/TODO/路线图和 host receipt；
- `docs/screenshots/` 只允许公开策略 README；
- 禁止常见私钥、token 和云凭据格式；
- 禁止未允许的邮箱以及真实用户 home/media 路径；
- 文本解码失败或可疑的大型系统快照必须显式处理，不能静默跳过。

正式发布额外使用 history 模式，要求所有历史作者邮箱使用 GitHub noreply，并确认历史对象
中不再包含私有路径。仓库首次公开前必须完成一次历史重写并通过该模式。

## 4. 文档门禁

公开文档遵循 [docs/README.md](README.md) 的路由：

- 每篇 living 文档不超过 200 行，AGENTS 保持短小；
- 本地链接必须存在且不能逃出仓库；
- 每个 crate 必须有 Role、Boundary、Contract and verification；
- 当前文档不能链接历史、评分、TODO 或真实回执；
- 规则变化直接重写正文，不追加日期流水。

## 5. 发布物门禁

tag 发布必须同时构建并验证全量 28 项产物（四端各 4 个 DEB/RPM 与 2 架构 MSI，加 Common 数据包）：

- 四端（G/I/T/B）与 Common 的 DEB 包：`dpkg-deb --info` 和 `--contents` 完整校验；
- 四端（G/I/T/B）与 Common 的 RPM 包：`rpm -qp --info`、`--list` 和 `--requires` 完整校验；
- 四端（G/I/T/B）Windows MSI 安装包：WiX 构建、MSI 数据库反编译和 payload 引用检查；
- 每个架构独立的 Linux/Windows SHA-256 清单（共 4 份清单）。

Windows 或 Linux 任一打包 job 失败，整个发布失败。所有必需文件验证完成后，单一 publish
job 才能创建或更新 GitHub Release，避免半成品发布。

## 6. 截图与真实环境

真实主机截图和回执只保存在忽略的 `.private/` 或 `target/` 中。公开截图必须来自确定性演示数据，并检查用户名、
进程、网络、设备、路径和元数据。无真实目标环境时只能报告 SKIP，不能把 fixture、编译或静态图片写成平台验证通过。

捕获后端按能力分类，不得用一个后端的成功替代另一个后端的语义验证：

- nested Niri 已建立 IPC socket 但 `niri msg` 超时：记 `BLOCKED (compositor/backend)`，既不判产品 PASS 也不判 FAIL，
  保留本地证据等待真实 compositor 或后端修复后重跑。
- gamescope 可作为单应用、固定输出尺寸的辅助像素捕获后端：只有真实 marker、PNG、source manifest 与独立验证器全通过，
  结果才覆盖 standalone 渲染与弹性布局审查；即使广播 `zwlr_layer_shell_v1` 也不能单独证明 layer surface 已合成——
  anchor、margin、exclusive zone、keyboard interactivity、output 选择、close/restart 与窗口管理仍须在目标 compositor
  验证，未建立可复现 receipt 前只能报 `SKIP`，不能冒充 Layer-Shell PASS。

## 7. 前端交互与像素证据

每个前端由同一套合同约束，证据通道按 toolkit 能力选择，全部 fail-closed：

| 前端 | headless 交互矩阵 | 真实像素证据 |
|---|---|---|
| GPUI | `scripts/parity/accept-frontend-interactions.sh gpui --scope linux`（standard `--with-gui`；内部仍调用 `scripts/accept-gpui-interactions.sh` 作为权威 runner 与回执作者） | `scripts/capture-niri.sh` / `capture-windows.sh` |
| Iced | `scripts/parity/accept-frontend-interactions.sh iced`（standard `--with-gui` / scoped `iced` 线） | `scripts/capture-iced.sh` |
| TUI | `scripts/parity/accept-frontend-interactions.sh tui`（standard `--with-gui` / scoped `tui` 线） | `scripts/capture-tui-matrix.sh`（矩阵）/ `scripts/capture-tui.sh`（单帧） |
| Bevy | `scripts/parity/accept-frontend-interactions.sh bevy`（standard `--with-gui`，Bevy 线 scoped standard 同样运行；内部仍调用 `scripts/accept-bevy-interactions.sh` 作为权威 runner 与回执作者） | `scripts/capture-bevy.sh`（Wayland-only） |

四个前端的画面证据统一默认 `TM_CAPTURE_NIRI_BACKGROUND=1`：由私有
`dbus-run-session`（无 service activation）和 `kwin_wayland --virtual` 承载 nested Niri，
按真实 PID/app-id/window-id 绑定窗口，使用
Niri `screenshot-window` 写出 PNG；验收脚本拒绝 `TM_CAPTURE_NIRI_BACKGROUND=0`，避免调试
参数意外触碰宿主桌面。需要研究可见 compositor 行为时，必须使用独立的手工调试环境，
不得把结果写入验收 receipt。
画面外观是可记录维度：scenario 表新增 `skin` 列（复用 `TM_SKIN` 词表），capture 把请求值传给应用并从 PNG 均值亮度回读 `appearance_rendered`，再按请求模式与回读结果比对得出 `appearance_control`：全部场景一致记 `honored`，任一场景不一致记 `ignored`，其余（含无法量测或不可判定项）记 `unknown`，不伪装成可控；Bevy/Iced/TUI 在 Performance/CPU 页各有 light/dark 成对场景（Bevy `performance`/`performance-dark`、Iced `cpu`/`cpu-light`、TUI 同页切换 `TM_TUI_CAPTURE_SKIN=gnome-light|gnome-dark`），保证同页可比。

每个后台 capture 必须经 `scripts/capture_supervisor.py` 获得随机 Run UUID，并将应用 binary、KWin 的
runtime/config/data/cache/state、Niri socket、D-Bus session 与 receipt 绑定到该 UUID；supervisor 使用用户 cgroup v2
和 detached watchdog 管理完整进程树。失败、取消或父进程消失都必须留下可审计 receipt 并回收 runtime；`latest` 只能由
通过独立 validator 的完整 run 在 `flock` 下以原子 pointer 发布。并发隔离由 `scripts/test_capture_isolation.py` 证明：
A/B 同时运行、窗口互不可见、单独终止 A 不得影响 B，主机 D-Bus/Wayland/KWin 状态不变，最终两个 run 都无进程和
runtime 残留。正式 RC7 本地收口必须以 `TM_CAPTURE_ISOLATION_GATE=1 bash scripts/quality/local-gates.sh standard`
重跑；CI/Rehearsal 若提供真实 Wayland/KWin runner，则同样强制该变量，普通无图形 runner 只能报告环境性 SKIP，
不能把它记为隔离 PASS。

发布指针除 `latest` 软链与 UUID 外还写结构化 `latest.json`，绑定 run、采集时刻与该 run 的
`source_manifest_sha256`；`scripts/evidence_status.py` 对当前树重算同一份前端作用域清单并比对，给出
`CURRENT`/`STALE`/`UNANCHORED` 的机械判定与差异文件清单，`--require-fresh` 可作门禁。参考态与候选态分离：
`scripts/evidence_promote.py` 把指定帧显式提升进 `target/<root>/reference/` 并拒绝静默覆盖。live 主机与编译
fixture 是两种数据制度，各占独立证据根（`screenshot-evidence` / `screenshot-evidence-demo`），跨前端对比必须在同一制度内进行。

Bevy 交互矩阵（统一矩阵 `scripts/parity/cross_frontend_matrix.tsv` 的 bevy 投影）由机械发现驱动：脚本先对 lib 目标做 nextest discovery，矩阵中的每个命名测试必须真实存在，然后完整运行 lib 目标；矩阵之外不存在"已登记但未运行"的用例。真实像素走嵌套 Niri，validator 对 app_id、PID/窗口身份、PNG、marker、source provenance 和当前 worktree fail-closed；无 compositor 时只报告 SKIP。

### 7.2 Bevy 光学玻璃证明

Bevy 生产材质桥在 `src/glass/optical/capability.rs` 声明能力上限；实际 capability 只由实时 `RenderAdapterInfo` 推导，无 adapter 时报告 `UNSUPPORTED`，主题立即返回 typed `Painted` 回退。quick 的 `bevy-optical-capability` 静态核对声明、门禁接线和 receipt 新鲜度；standard 的 `bevy-optical-gpu` 用 nextest discovery 确认被忽略的 `production_shader_renders_optical_effects_and_tracks_the_backdrop` 仍存在，再强制运行它，把 blur、refraction、dispersion、specular、grain、opacity、rounded clipping 和 lower-scene tracking 的通过结果写入 `target/optical-gpu-evidence/receipt.json`；`bevy-optical-capability-receipt` 复核同一源码摘要。测试打印 adapter 身份和操作清单，receipt 区分 `PROVEN`、`SOFTWARE_ADAPTER`、`NO_ADAPTER`、`SKIPPED`、`REGRESSION`；无 adapter 或软件 adapter 明确记录 `PAINTED`，不标记为光学证明，`TM_OPTICAL_GPU_GATE=1` 在二者下 fail-closed。receipt 的 `max_nested_layers=1` 与 `nesting_layers_proven=false` 是硬事实：当前只证明单层 lower composite，嵌套玻璃仍由主题 typed fallback 拒绝，不得重新贴标签。

UI 边界改动由 `scripts/quality/ui-evidence-route.sh` 按真实渲染消费面路由：合同测试树、声明模块与 dev-only 前端测试树只要求 headless 交互矩阵，不索取像素回执；被真实渲染消费的合同输入模块与产品前端才在 `--require-capture` 下按消费端要求新鲜回执，未列出的 `ui-contract` 路径保持四端 fail-closed。`locales/*` 不再只按路径粗判：`scripts/quality/locale_keyset_classifier.py` 读取目录**内容**——仅当整键增删、无存活键值变更、目录文件集与跨目录键关系不变时才判为 key-set-only（只走 headless）；任一已存键值改变、形状/对称变化、目录不可解析、缺失一侧，或改动集含非目录路径，都 fail-closed 到四端。路径分类表与内容分类器均由 `scripts/quality/ui_evidence_route_guard.py` 机械钉住（quick）。

真实捕获的 provenance 与当前工作树绑定：validator 比较 git head、rustc 版本与 worktree clean/dirty 三元组；捕获期间工作树由 clean 变 dirty 会让校验失败且该次捕获不发布，dirty 状态下三元组不变即通过：捕获必须单跑并等待安静的稳定窗口，失败后在同一稳定状态重跑，不修改 validator。

### 7.3 弹性布局收口协议

所有响应式、密集详情、图表分组或固定 viewport 改动必须先遵循 [ELASTIC_LAYOUT_PLAYBOOK.md](ELASTIC_LAYOUT_PLAYBOOK.md)：由根预算推导完整 slot footprint 与 mandatory/elastic/optional 分配，lower band 只能整组准入、摘要或隐藏，底部安全带与右侧 label/value bounds 必须有 headless 断言；随后运行当前构建的真实 capture 并通过独立 validator，逐页覆盖最小、正常、tall、wide-short、窄宽与长文本状态。单张截图、fixture 或编译通过不得替代这套证据；推导步骤与状态矩阵以 playbook 为唯一权威，本节只规定收口必须留下的证据。

## 8. 验证器质量

验证器必须自动发现范围、执行目标、检查结果和副作用，并在范围为空、解析失败或回执不完整时 fail-closed。禁止用源码字符串存在性、恒真断言、固定测试数量或 `echo PASS` 证明行为。

- 静态策略守卫的禁用 token 列表按（去行注释后的）精确子串匹配生产源，夹具文本必须避开；发生碰撞时修改夹具（例如改用真实设备节点名），不得放宽或绕过守卫。
- **探测器的可见范围就是它的结论边界。** 读 Rust 的探测与普查门不得声称自己读懂了它匹配不到的代码：签名解析由唯一的平衡扫描器承担（以 `[^)]*` 取参数表、以 `<[^>]*>` 取泛型表会把带嵌套括号的签名整条漏掉，从而把"未复查"报成"无重复"），新增门一律复用该权威并保留"嵌套签名仍被看见"的夹具。逐条比对（增量/差集）门必须记录并登记探测器身份，身份未登记即拒绝比较；只改扫描逻辑同样轮换身份。
- **消失与删除都必须可归因、可证伪。** 已登记的命中从报告消失只有三条合法通道：确实被推掉（附落地证据）、被明确改判、或探测器自身变更（附涉及条目与理由/触发器）；声称"已推掉"却仍在最新报告出现的回执必须反向判红。"共享 owner 已存在""已分类""不适用"都不是删除条目的理由，此类条目改为显式类别呈现，由裁决决定 KEEP/RETIRE。报告新增任何条目类别时，判定器必须在同一变更内读取并裁决它——出现无人读取的类别即硬失败，"pass" 不得建立在"没读"之上。

## 9. 并行隔离验证与视觉对等（标准，2026-08-29 起）

**并行隔离验证**（`scripts/verify-isolated.sh <line> [gate]`，各行通用）：行门禁只在本行自己的差异上跑——脚本按本行
足迹清单（`scripts/isolated-paths/<line>.txt`）把本行未提交改动（含 intent-to-add 未跟踪文件）应用到 HEAD 私有
worktree 内执行 fmt/clippy/nextest/capture，主 checkout 永不被验证流程改动。这样兄弟线在共享 checkout 里的未提交
重构（哪怕暂时编译不过）永远不阻塞本行门禁；本行也不得因他行撕裂而宣布 SKIP。足迹清单是行声明：本行拥有或改动的
路径（含共享 crate 的行内改动）必须全部列入，隔离验证才携带全部本线事实。Cargo 锁回退沿用全仓约定（`TM_CARGO_LOCK`
空值 = 解锁，未设 = `--locked`）。

**视觉对等目检**（渲染行收口门禁）：绿色门禁只证明行为与来源，不证明观感。渲染行每轮收口必须把本行捕获与参考行捕获
逐页并排目检，差距逐条记账——修掉的进提交，暂缓的写入该行公开文档的"已知边界/存异"清单（如
`crates/taskmanager-bevy-ui/docs/BEVY_UI_FRONTEND.md`）。装饰禁止用文本字形冒充（嵌入字体无该字形即渲染 tofu）；图标走
语义注册表（`IconId` → 共享 SVG → 各 toolkit 材质化），行门禁须含机械反字形扫描与有界行单行契约测试。
