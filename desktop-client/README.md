# Sub2API Desktop Client

`desktop-client` 是面向终端用户的 `Slint + Rust` 桌面客户端。当前已经具备亮色桌面应用壳、登录/注册/找回密码闭环、官方模式安装检测与启动、平台模式受管 `CODEX_HOME` 基础、Windows 安装包脚本与安装验证链路。

## 本地运行

推荐从仓库根目录运行隐藏窗口入口：

```powershell
.\start-desktop-client.vbs
```

如果需要查看 Cargo 输出，可直接运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

如果是在 VMware / Hyper-V 一类虚拟机里调试，客户端会在检测到虚拟机 BIOS 指纹时自动切到 `SLINT_BACKEND=winit-software`，尽量避开虚拟显卡导致的窗口初始化闪退。安装包直启时的启动诊断会写到 `%LOCALAPPDATA%\sub2api\TokenClient\data\logs\startup.log`。

停止正在运行的客户端：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\stop-desktop-client.ps1
```

## 验证

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
```

## Windows 安装包

在仓库根目录运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "https://your-sub2api.example.com"
```

如果要在打包时写入固定后端地址：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "https://your-sub2api.example.com/api/v1"
```

也可以直接传站点根地址，脚本会自动补成 `/api/v1`：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "https://your-sub2api.example.com/"
```

脚本会：

- 生成 `desktop-client\target\release\sub2api-desktop.exe`
- 生成 Inno Setup 安装包 `dist\desktop-client\Sub2API-Desktop-Setup-<version>.exe`
- 生成对应的 `SHA256` 校验文件
- 在 Windows 下为 exe 嵌入版本信息和图标

如果你已经有代码签名证书，可以继续运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\sign-desktop-installer.ps1 -CertThumbprint "<thumbprint>"
```

或：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\sign-desktop-installer.ps1 -PfxPath "C:\certs\codesign.pfx" -PfxPassword "<password>"
```

这里临时使用 `rsproxy.cn` 是因为当前机器访问 crates.io 出现 TLS 握手失败；如果你的网络能直连 crates.io，可以去掉两个 `--config` 参数。

注意：安装包打包时现在必须显式传入 `-ApiBaseUrl`。这样可以避免把默认的本机调试地址 `127.0.0.1:8080` 意外写进面向用户的安装包。

## 当前边界

- 已有：亮色桌面应用壳、基础路由枚举、账号认证 JSON 契约、2FA 登录响应契约、`/auth/me`、`/groups/available`、`/redeem`、`/redeem/history`、`/subscriptions/summary`、desktop session create/refresh/revoke 调用层、HTTP 错误保真、系统凭据存储接口、登录/注册/邮箱验证码/忘记密码/CDK 兑换 UI 与主程序接线、Codex Desktop/CLI 安装检测、官方模式启动、平台代理模式启动、受管 home/runtime 元数据、平台会话自动续期与退出回收、Windows 安装包脚本与 SHA256 输出。
- Windows 边界：Windows Store 安装的 Codex Desktop 官方模式仍使用 `shell:AppsFolder`；平台代理模式会改为直接启动包内 `app\Codex.exe` 并注入独立 `CODEX_HOME`，避免污染用户官方配置。
- 未接入：订单明细、订阅明细页、真实证书签名执行、生产 API 地址固化后的最终发布包。
- 安全约束：后续 UI 仍不展示连接密钥、服务地址或 runtime 凭证；refresh token 默认走系统凭据存储，文件实现仅作为测试替身。
