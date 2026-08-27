# ADR-035: Windows foreign-process control 的 UAC/native helper 传输

- 状态：已接受（设计契约）
- 相关：`adr/023-per-feature-privilege-escalation-framework.md`、
  `adr/031-windows-native-safe-boundary.md`、
  `docs/PERMISSION_MODEL.md`、
  `crates/taskmanager-escalation/README.md`、
  `crates/taskmanager-platform-windows/README.md`

## 背景

Windows 上对不同用户或受保护进程的控制不能依赖普通继承 token 子进程，
也不能把常驻 SYSTEM 服务或 blanket capability 放进主程序。当前生产行为
对尚未接线的 foreign-process control 保持 typed `Unsupported`；本 ADR 只
冻结未来传输的信任边界，不把设计当成已交付功能。

## 决策

1. 采用 Windows 原生 `ShellExecuteExW` 的 `runas` verb 与
   `SEE_MASK_NOCLOSEPROCESS`，每次操作启动一次固定 helper，并使用有界等待。
   不使用 PowerShell、CMD、`sudo` 或常驻服务。
2. 不新增第五个 unsafe boundary。`ShellExecuteExW`、有界
   `WaitForSingleObject`、`GetExitCodeProcess` 和 `CloseHandle` 进入既有
   `taskmanager-windows-api` 边界；公开 API 只返回 owned/typed 值与 typed error。
3. helper 复用 `taskmanager-process-control-helper`，接收固定参数：目标 PID、
   creation token 和单一操作。helper 在 elevated 侧重新打开目标并复核 creation
   token，PID 不能单独作为授权凭据。
4. 结果通过每次调用独立、随机命名且 ACL 受限的命名管道或临时文件返回；回传
   必须有界、校验 schema 和目标身份。协议数据不通过 stdout 隐式解释。
5. 应用侧先尝试直连；只有明确的权限失败才进入 escalation。返回后再次复核
   token；目标消失或 PID 复用统一成为 `IdentityChanged`，不得误操作替换进程。

## typed outcome

| 传输事实 | 结果 |
|---|---|
| helper 验证身份并完成恰一操作 | `Applied` |
| token 不符或目标消失 | `Failed { IdentityChanged }` |
| elevated helper 仍被拒绝 | `Failed { PermissionDenied }` |
| UAC 明确拒绝（`ERROR_CANCELLED`） | 可重试的 `PermissionDenied` |
| 无交互会话或授权无法归因 | `AuthorizationUnavailable` |
| helper 未安装或跨界超时 | `HelperUnavailable` |
| 回传非契约内容 | `HelperProtocolViolation` |
| transport 尚未接线或平台不支持 | `Unsupported` |

取消、超时、授权未完成、helper 缺失和协议违约必须互斥；任何一种都不能
折叠成成功或普通 `PermissionDenied`。无头测试不得伪造 UAC 同意。

## 当前发布边界

传输尚未进入生产接线，Windows foreign-process control 仍返回
`Unsupported`。Windows MSI 只安装 GPUI 应用和 LICENSE，不安装该 helper、
服务或自启动项；Windows 签名与打包策略不构成 UAC 行为已经完成的证明。

## 验证

契约测试覆盖当前 `Unsupported`、身份复核、失败分类和 stale completion 拒绝。
未来接线必须增加 Windows 原生 API 的安全边界审计、固定 helper 的安装清单、
成功/拒绝/超时/无提示/协议损坏/PID 复用的行为测试，以及真实交互桌面验证。
Linux CI、fixture、交叉编译和 MSI 文件存在性检查都不能替代这些验证。
