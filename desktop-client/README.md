# Sub2API Desktop Client

`desktop-client` 是面向终端用户的 `Slint + Rust` 桌面客户端骨架。当前阶段只完成可编译启动壳、路由枚举和首屏 UI，用于承接后续登录、CDK、安装探测和 Codex 启动编排。

## 本地运行

推荐从仓库根目录运行隐藏窗口入口：

```powershell
.\start-desktop-client.vbs
```

如果需要查看 Cargo 输出，可直接运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

停止正在运行的客户端：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\stop-desktop-client.ps1
```

## 验证

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib app_bootstrap_exposes_router_module
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
```

这里临时使用 `rsproxy.cn` 是因为当前机器访问 crates.io 出现 TLS 握手失败；如果你的网络能直连 crates.io，可以去掉两个 `--config` 参数。

## 当前边界

- 已有：Slint 主窗口、基础路由枚举、启动/停止脚本。
- 未接入：登录、注册、忘记密码、CDK、安装探测、桌面会话、Codex 启动。
- 安全约束：后续 UI 仍不展示 API Key、Base URL 或 runtime token。
