# Desktop Client 定版交付说明

- 日期：2026-04-19
- 适用目录：`客户端壳子/`
- 目的：把“可交付源码”、“调试/草稿资产”、“交接文档”分层，便于继续套壳和交付

## 定版结论

`客户端壳子/desktop-client` 当前可以视为桌面客户端源码主目录。

本轮定版时，真正应被当作“正式源码 / 正式交付”的内容只有这些：

- `客户端壳子/desktop-client/src`
- `客户端壳子/desktop-client/ui`
- `客户端壳子/desktop-client/assets`
- `客户端壳子/desktop-client/packaging`
- `客户端壳子/desktop-client/tests`
- `客户端壳子/desktop-client/Cargo.toml`
- `客户端壳子/desktop-client/build.rs`
- `客户端壳子/build-desktop-installer.ps1`
- `客户端壳子/start-desktop-client.ps1`
- `客户端壳子/start-desktop-client.vbs`
- `客户端壳子/stop-desktop-client.ps1`
- `客户端壳子/docs`

## 调试 / 草稿资产

以下内容不应被当作正式交付源码，它们是设计过程、草稿或本地辅助工具：

- `客户端壳子/desktop-client/analyze_layout.py`
- `客户端壳子/desktop-client/refactor.py`
- `客户端壳子/desktop-client/scratch_screen1.html`
- `客户端壳子/desktop-client/scratch_screen2.html`
- `客户端壳子/desktop-client/stitch_designs/`
- `客户端壳子/desktop-client/备份/`
- `客户端壳子/desktop-client/doc/`
- 根目录 `tmp_vm_*`

这些文件可以保留做参考，但不属于正式交付物。

## 交接文档入口

继续阅读顺序建议如下：

1. [DESKTOP_CLIENT_CN.md](D:/挣钱/token/token客户端/客户端壳子/docs/DESKTOP_CLIENT_CN.md)
2. [desktop-client-handoff.md](D:/挣钱/token/token客户端/客户端壳子/docs/superpowers/handoff/2026-04-19-desktop-client-handoff.md)
3. [desktop-server-customization-contract-cn.md](D:/挣钱/token/token客户端/客户端壳子/docs/superpowers/handoff/2026-04-19-desktop-server-customization-contract-cn.md)
4. [desktop-gui-vm-test.md](D:/挣钱/token/token客户端/客户端壳子/docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md)
5. [server-incremental-update-cn.md](D:/挣钱/token/token客户端/客户端壳子/docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md)

## 当前源码边界

### 已经可以作为定版基线的部分

- 登录 / 注册 / 找回密码 / 重置密码
- 一级总览与二级详情布局
- 启动中心与分组选择
- CDK、计费中心、设置帮助等信息架构
- CLI 代理链路
- Desktop Runtime / Store Desktop 启动相关修复
- 安装包脚本与启动脚本

### 仍在继续验证的部分

- VMware 中 `Codex Desktop` 的最终可见 GUI 问答闭环

这不影响“源码定版整理”，但会影响“已完成全部桌面端真实可见问答取证”的表述。

## 推荐交付口径

对外或对接下游团队时，建议这样描述：

- 这是当前 Windows 客户端的定版源码基线
- UI 结构、启动流程和服务端对接已经整理清楚
- 后续套壳优先改 `ui/`，不要先改 `platform/`
- VMware 中的 Desktop 最终可见问答取证仍在补最后一步

## 后续套壳建议

套壳时优先改：

- `客户端壳子/desktop-client/ui/app-window.slint`
- `客户端壳子/desktop-client/ui/components/brand_panel.slint`
- `客户端壳子/desktop-client/ui/screens/*.slint`

尽量不要先改：

- `客户端壳子/desktop-client/src/platform/install_detection.rs`
- `客户端壳子/desktop-client/src/platform/launcher.rs`
- `客户端壳子/desktop-client/src/main.rs`

因为这些是已经踩坑并验证过的平台逻辑。
