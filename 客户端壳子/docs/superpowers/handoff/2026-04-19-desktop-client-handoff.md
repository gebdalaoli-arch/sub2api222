# Desktop Client Handoff

- 日期：2026-04-19
- 交接范围：`desktop-client` 当前工程状态、关键入口、平台链路、后续套壳接点
- 关联分支：`codex/sub2api-desktop-client-v1`

## 当前结论

这版 `desktop-client` 已经不是原型壳，而是一个具备完整主流程的 Windows 桌面控制器：

- 账户流程：登录、注册、验证码、忘记密码、重置密码
- 启动流程：官方 Desktop / CLI，平台代理 Desktop / CLI
- 计费摘要：余额、并发、订阅摘要、兑换记录、订单列表摘要、CDK 兑换
- 平台会话：desktop session 创建、续期、回收，受管 `CODEX_HOME`
- Windows 打包：Inno Setup 安装包、SHA256 输出、可选签名脚本
- 虚拟机兼容：VMware / 通用虚拟机指纹触发 `SLINT_BACKEND=winit-software`

当前最值得注意的边界：

- `Codex CLI` 的平台代理真实问答和扣费链路已经在 VMware 中验证通过
- `Codex Desktop` 的 Windows Store 平台代理路径已经定位到一个真实问题：直接执行 `WindowsApps\...\app\Codex.exe` 在客机里会触发 `os error 5`
- 为此，当前分支已把 Windows Store Desktop 的平台代理启动路径改为 shell 启动方式，后续还需要继续做 GUI 可见问答闭环复测

## 关键提交

- `b005039 fix: stabilize desktop startup in virtual machines`
- `d39a28e fix: block localhost api in packaged desktop builds`
- `fad9b1b fix: remove embedded oauth client secrets`
- `8a24eb1 fix: support store desktop platform launch`
- `9db9055 fix: persist desktop runtime usage keys`

## 工程入口

## 1. 主程序接线

- 入口文件：[desktop-client/src/main.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/main.rs)
- 这里负责：
  - 初始化 Slint 窗口
  - 恢复本地状态与 refresh token
  - 绑定登录/注册/密码恢复/CDK 回调
  - 绑定官方启动与平台代理启动回调
  - 启动 session 续期与退出回收线程

如果后续要套壳、改布局、换主题，应该尽量避免在这里重写业务逻辑，只调整触发关系和 UI 绑定。

## 2. 平台链路入口

- 安装探测：[desktop-client/src/platform/install_detection.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/install_detection.rs)
- 启动器：[desktop-client/src/platform/launcher.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/launcher.rs)
- 受管 home：[desktop-client/src/platform/managed_home.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/managed_home.rs)
- 虚拟机软件渲染兜底：[desktop-client/src/platform/runtime_bootstrap.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/runtime_bootstrap.rs)
- 运行时会话模型：[desktop-client/src/platform/runtime_session.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/runtime_session.rs)

这几个文件一起决定“本地装了什么、怎么起、往哪里写 `CODEX_HOME`、什么时候回收”。

## 3. API 契约入口

- 认证：[desktop-client/src/api/auth.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/auth.rs)
- 账户数据：[desktop-client/src/api/account.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/account.rs)
- 分组：[desktop-client/src/api/groups.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/groups.rs)
- Desktop Session：[desktop-client/src/api/desktop_sessions.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/desktop_sessions.rs)
- 兑换与订单：[desktop-client/src/api/redeem.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/redeem.rs), [desktop-client/src/api/payment.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/api/payment.rs)

如果后续外壳只换 UI，不建议碰这些契约文件。

## 4. 状态与凭据

- 普通状态：[desktop-client/src/storage/app_state.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/storage/app_state.rs)
- 系统凭据存储：[desktop-client/src/storage/secure_store.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/storage/secure_store.rs)

约束：

- refresh token 默认走系统凭据存储
- 文件型 credential store 只是测试替身
- 后续套壳不能把后端地址、runtime token、refresh token 暴露回 UI

## UI 层与套壳建议

## 1. 先动哪里

后续如果要“把客户端外壳套上”，建议优先从 Slint 视图层动，不要直接从平台链路层开刀：

- [desktop-client/ui/app-window.slint](D:/挣钱/token/token客户端/客户端壳子/desktop-client/ui/app-window.slint)
- [desktop-client/ui/screens/login.slint](D:/挣钱/token/token客户端/客户端壳子/desktop-client/ui/screens/login.slint)
- [desktop-client/ui/screens/dashboard.slint](D:/挣钱/token/token客户端/客户端壳子/desktop-client/ui/screens/dashboard.slint)
- [desktop-client/ui/screens/launch_panel.slint](D:/挣钱/token/token客户端/客户端壳子/desktop-client/ui/screens/launch_panel.slint)
- [desktop-client/ui/screens/about.slint](D:/挣钱/token/token客户端/客户端壳子/desktop-client/ui/screens/about.slint)

这些文件决定视觉外壳、布局和文案层级，适合优先做品牌替换。

## 2. 暂时不要重写哪里

以下文件承载的是已经踩过坑的平台逻辑，套壳阶段不要轻易重写：

- [desktop-client/src/platform/launcher.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/launcher.rs)
- [desktop-client/src/platform/install_detection.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/install_detection.rs)
- [desktop-client/src/platform/runtime_bootstrap.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/runtime_bootstrap.rs)
- [desktop-client/src/main.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/main.rs)

如果必须改，建议只做局部包装，先保留原行为。

## 3. 套壳推荐顺序

1. 先定品牌色、标题、左侧导航和首页排版
2. 再改登录页和计费页外观
3. 最后改启动中心视觉
4. 平台启动、安装检测、desktop session 写入与回收逻辑保持不动

## Windows / VMware / Store 特殊说明

## 1. VMware

- 客户端会根据 BIOS 指纹自动切到 `SLINT_BACKEND=winit-software`
- 启动日志位置：
  - `%LOCALAPPDATA%\sub2api\TokenClient\data\logs\startup.log`
- 这部分不要删，否则虚拟机启动闪退问题会回归

## 2. Windows Store Desktop

- 官方模式：`shell:AppsFolder`
- 平台代理模式：
  - 旧路径：直接执行 `WindowsApps\...\app\Codex.exe`
  - 已定位问题：VMware 客机中会触发 `拒绝访问 (os error 5)`
  - 当前分支修复方向：改走 shell 启动路径并保留隔离 `CODEX_HOME`

## 3. CLI

- CLI 是当前最稳的平台代理验证路径
- 虚拟机里已经完成真实问答与扣费验证

## 构建与验证入口

## 1. 本地运行

从仓库根目录：

```powershell
.\start-desktop-client.vbs
```

调试：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

## 2. 测试

推荐：

```powershell
cargo --config "source.crates-io.replace-with='rsproxy-sparse'" --config "source.rsproxy-sparse.registry='sparse+https://rsproxy.cn/index/'" test --manifest-path desktop-client/Cargo.toml --lib
```

说明：

- 当前分支存在一个与这次修复无关的旧 UI 文案断言失败，全量 `cargo test` 不是完全绿色
- 与 Store Desktop 启动修复直接相关的 `platform::launcher` 定向测试已通过

## 3. 打包

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "http://43.173.88.95:8080/"
```

## 交接建议

后续接手时，先看这几份文档：

1. [docs/DESKTOP_CLIENT_CN.md](D:/挣钱/token/token客户端/客户端壳子/docs/DESKTOP_CLIENT_CN.md)
2. [desktop-client/README.md](D:/挣钱/token/token客户端/客户端壳子/desktop-client/README.md)
3. [docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md](D:/挣钱/token/token客户端/客户端壳子/docs/superpowers/verification/2026-04-19-desktop-gui-vm-test.md)
4. 本文档

这样能先知道：

- 客户端是什么
- 现在已经通了哪些链路
- VMware / Store 的坑在哪里
- 套壳应该从哪一层开始

