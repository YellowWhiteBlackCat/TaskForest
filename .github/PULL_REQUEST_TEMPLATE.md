## 变更说明

<!-- 一段话说明目的与方案；引用相关 issue 或 ADR -->

## 影响面

- 受影响 crate / 前端：
- 平台影响：Linux / Windows / macOS / 无平台差异
- 是否改变公开契约或用户可见行为：是 / 否（涉及时列出同步更新的文档）

## 自查清单

- [ ] `bash scripts/quality/local-gates.sh quick` 通过（Windows 用 `scripts/windows/local-gates.sh`）
- [ ] 行为变化附带行为测试；不含源文本断言、空断言或宿主特定值
- [ ] 提交信息符合 Conventional Commits，scope 使用领域/crate 名
- [ ] 未包含私有材料：私有路径、真实截图、主机回执、凭据、绝对用户路径
