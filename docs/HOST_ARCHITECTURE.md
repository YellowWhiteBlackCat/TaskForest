# TaskForest 宿主与 surface 架构

本文定义当前的宿主边界：同一 core/application 可以组合多套系统适配器和多套 UI；每套
图形 UI 可以并存 standalone 与 Wayland layer-shell 两种 surface host。业务投影、页面
语义和主题合同不因窗口角色复制。

## 1. 所有权

```text
core → application → shell/projection
                         ↓
              selected frontend projection
                         ↓
           standalone host | layer-shell host
                         ↓
                 native platform adapter
```

- `core` 只拥有领域事实和纯规则。
- `application` 只拥有命令、reducer、port 和应用生命周期。
- `shell` 和 `ui-contract` 只拥有 renderer-neutral projection、交互和语义合同。
- `taskmanager-app-host` 组合 runtime、native client 和工具包无关的 surface role 请求。
- GPUI、Iced、Bevy 各自拥有 event loop、window/surface、renderer state 和 native adapter。
- `taskmanager-platform-linux` 保持 telemetry/control adapter 责任，不转为 UI surface owner。

## 2. Surface role 合同

`taskmanager-app-host` 发布 `WindowPresentation`：

- `Standalone`：现有普通桌面窗口路径；
- `LayerShell(LayerShellSpec)`：仅由 layer、anchor、size、margin、exclusive zone、
  keyboard interactivity、output hint、namespace 和 fallback policy 组成。

合同只包含拥有所有权的 Rust 值，不包含 `wl_surface`、`wl_output`、Wayland event queue、
raw-window-handle 或任何 toolkit 类型。`LayerShellSpec` 在交给 adapter 前必须通过 typed
validation；`exclusive_zone` 只接受 `-1` 或非负值，namespace 不能为空。

Surface role 是 per-surface，而不是全局 frontend 模式。主窗口和监控面板可以在未来拥有
不同 role；首个实现也可以只创建一个 surface。

## 3. 两条宿主路径

每个图形 frontend 都保持两条内部路径：

| frontend | standalone host | layer-shell host |
|---|---|---|
| GPUI | 现有 GPUI Wayland/desktop path | GPUI 自有 Wayland client 的 layer surface path |
| Iced | 现有 `iced_winit` path | 独立 runner、fork 或受控第三方 shell |
| Bevy | 现有 `bevy_winit` path | 独立 window plugin/runner 和事件适配 |

两条路径共享 application client、shell track、projection、主题和页面语义；不共享各自
toolkit 的窗口对象、事件循环或 renderer surface 生命周期。

## 4. Layer-shell 边界

Layer-shell 是可选的 Wayland compositor capability。adapter 必须先 probe
`zwlr_layer_shell_v1`，再创建新的 layer surface；普通 `xdg_toplevel` surface 不能原地
切换 role。协议的 configure/ack、buffer 提交、`closed`、output disconnect、缩放和
输入状态都由对应 frontend adapter 负责。

layer-shell 不承诺普通窗口的标题栏、移动、最大化、最小化和装饰操作。adapter 必须返回
真实 capability 或 typed unavailable/fallback，不得把不支持的操作映射为静默成功。

## 5. Fallback 与平台

layer-shell 请求默认允许回退到 standalone；严格模式可以选择 typed unavailable。没有
Wayland、没有 layer-shell global、协议版本不满足或 compositor 关闭 surface 时，均不能
伪造 layer-shell 已生效。

Windows、macOS、X11 和不提供该 global 的 Wayland compositor 继续使用各自 standalone
host。layer-shell namespace 是 layer surface 的身份；普通窗口的 app-id 规则不能替代它。

## 6. 当前 GPUI vertical slice

GPUI 已接入第一条可选 layer-shell host。默认启动和 capture 仍是完整的 standalone
应用，不会进入 widget 分支；Linux/Wayland 开发者显式设置
`TASKFOREST_WINDOW_HOST=layer-shell` 后，才请求一个 Top、右上锚定、`520×360`、16px
边距、non-exclusive 的桌面 widget。GPUI 先探测 global，再执行空 commit → configure →
ack → buffer commit；实际 configure 尺寸会同步到 renderer 和 viewport。widget 只渲染
共享 Dashboard projection 的紧凑 CPU、Memory、Processes、Alerts 卡片，不复用普通窗口
的 titlebar、导航和长页面布局。

global、版本、输出或配置不可用时按 `LayerShellSpec::fallback` 回到普通窗口或返回错误。
现阶段 Iced/Bevy 仍只保留 standalone host；layer role 也不承诺移动、最大化、最小化和
server decoration 等普通窗口能力。

## 7. 原生代码与依赖

公共合同和业务 crate 保持 safe Rust。原始 Wayland protocol、GPU raw handle 和 event-loop
生命周期只能位于受审计的 native boundary 或对应 frontend 的明确 adapter 中。不得把
Wayland 依赖通过 transitive dependency 假定为公共合同，也不得把 raw object 传入 core、
application、shell 或 `taskmanager-ui-contract`。

## 8. 验证合同

- 无 capability 时验证 typed fallback/unavailable；
- standalone 与 layer-shell 分别验证创建、configure、resize、close 和退出；
- layer-shell 验证 anchor、zero size、exclusive zone、keyboard mode、output 选择、
  output disconnect 和 compositor restart；
- 前端页面验证同一 projection 与命令语义，不因 host 分支产生第二份业务状态；
- 真实 compositor 证据只能来自对应 Wayland 环境，fixture 不能替代平台验证。

不可逆的分叉边界记录在 [ADR-037](../adr/037-parallel-frontend-hosts-and-layer-shell-contract.md)。
