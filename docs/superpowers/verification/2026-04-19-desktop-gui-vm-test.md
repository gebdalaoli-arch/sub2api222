# Desktop GUI VM Test Record

- 日期：2026-04-19
- 环境：VMware Workstation / Windows 11 客机 / 用户 `vmuser`
- 目标：补齐 `Codex Desktop` GUI 可见问答证据

## 已确认链路

## 1. 已通过

- 客户端在 VMware Windows 11 客机中可以启动，不再闪退
- `123@123.com / 112233` 可在客机里登录客户端
- `Codex CLI` 真实问答与服务端扣费已经验证通过
- 服务端 Desktop Runtime 请求与扣费链路已验证通过
- `usage` 记录缺失的后端根因已经修复为通用逻辑，不只针对 `123@123.com`

## 2. 本轮新发现

- 客机里旧安装器弹窗会污染桌面取证，必须先清理
- 平台代理 Desktop 在 Windows Store 版 `Codex Desktop` 下会报：

```text
平台代理模式启动失败：拒绝访问。 (os error 5)
```

- 根因已定位到旧实现直接执行：

```text
C:\Program Files\WindowsApps\OpenAI.Codex_...\app\Codex.exe
```

在 VMware 客机中该路径会触发访问拒绝

- 当前分支已把 Windows Store Desktop 的平台代理模式改为 shell 启动路径

## 本轮证据文件

- 启动中心错误状态导出：
  - [desktop-result-dump.txt](D:/codex_data/vm-artifacts/desktop-result-dump.txt)
- 启动按钮执行结果：
  - [dashboard-to-desktop.txt](D:/codex_data/vm-artifacts/dashboard-to-desktop.txt)
- `Codex Desktop` 包目录与日志扫描：
  - [codex-package-scan.txt](D:/codex_data/vm-artifacts/codex-package-scan.txt)
  - [codex-profile-scan.txt](D:/codex_data/vm-artifacts/codex-profile-scan.txt)
- 最新 `Codex Desktop` 日志：
  - [codex-desktop-latest-t0.log](D:/codex_data/vm-artifacts/codex-desktop-latest-t0.log)
  - [codex-desktop-latest-t1.log](D:/codex_data/vm-artifacts/codex-desktop-latest-t1.log)
- 屏幕抓图：
  - ![安装器占用弹窗](/D:/codex_data/inline-images/vm-codex-screen.png)
  - ![清理后桌面](/D:/codex_data/inline-images/vm-after-cleanup.png)
  - ![重启后桌面](/D:/codex_data/inline-images/vm-after-exe-replace.png)
  - ![再次登录后桌面](/D:/codex_data/inline-images/vm-after-vm123-login.png)

## 当前判断

当前 VMware 客机里的 Desktop GUI 问答闭环还没有完全补齐，原因不是单点：

1. 旧安装器文件占用弹窗干扰了前台桌面状态
2. Windows Store Desktop 的平台代理旧实现确实存在 `os error 5`
3. 覆盖新 exe 后，客机里 `Sub2API` 的可见窗口与脚本导出的状态文件仍存在时序不一致，需要继续做一次干净的“启动后立即取证”

## 截止本记录的状态

- CLI：真实问答 + 扣费，已完成
- Desktop Runtime 服务端请求：已完成
- Desktop GUI 可见问答：仍在继续补测

## 下一步建议

1. 在当前分支修复后的新 exe 上继续客机复测 `平台模式 Desktop`
2. 若 shell 启动路径不再报 `os error 5`，继续补 GUI 输入与返回文本取证
3. 若仍无可见窗口，则进一步围绕 `Codex Desktop` 自身 Store 包窗口激活问题继续排查
