# Sub2API Desktop Client 全面说明

## 1. 项目定位

`Sub2API Desktop Client` 是面向终端用户的 Windows 桌面客户端，目标不是把后台管理面板搬到桌面上，而是把下面三件事做成一个低认知成本、可交付、可维护的本地客户端：

1. 账号闭环：登录、注册、邮箱验证码、忘记密码、重置密码、2FA。
2. 启动入口：统一承接 Codex Desktop / Codex CLI 的官方模式与平台代理模式。
3. 计费入口：优先支持 CDK 兑换、订阅摘要、订单/兑换记录查看，充值页暂时作为公告区。

客户端当前采用亮色原生桌面方向，不暴露任何 API 地址、平台密钥或桌面会话 runtime token。

## 2. 当前交付结论

截至 `2026-04-18`，这版客户端已经具备以下可交付能力：

- 已完成桌面应用壳、导航、账户流程、启动中心、CDK/订阅工作台。
- 已完成 Windows 安装包打包链路，可生成可分发的安装包。
- 已完成对 Sub2API 服务端关键接口的桌面端契约接入。
- 已完成 Codex 安装检测、官方启动、平台代理 CLI 启动基础链路。
- 已完成平台代理受管 `CODEX_HOME`、desktop session 续期与回收。
- 已为 VMware / 通用虚拟机图形环境补上启动早期软件渲染兜底与本地启动日志。
- 已明确 Windows Store 版 Codex Desktop 的产品边界：只支持官方模式，不支持本次启动级别的平台代理注入。

当前不把这版定义为“最终商业正式版”，但已经是“可安装、可演示、可继续灰度测试”的桌面客户端基础版本。

## 3. 产品结构

客户端 UI 目前按四个主区组织：

### 3.1 账户中心

- 邮箱 + 密码登录
- 邮箱验证码注册
- 二步验证登录
- 忘记密码
- 重置密码

这部分由主程序统一调度异步请求，并在成功后同步拉取用户资料、可用分组、订阅摘要、兑换记录和订单列表。

### 3.2 启动中心

提供四类启动路径：

1. 官方 Desktop
2. 官方 CLI
3. 平台代理 Desktop
4. 平台代理 CLI

但在 Windows 上需要区分：

- Windows Store 安装的 Codex Desktop：仅支持官方模式
- Codex CLI：支持平台代理模式

客户端会先做本机安装检测，再按目标类型决定是否允许进入平台代理启动。

### 3.3 计费与公告

当前计费工作台支持：

- CDK 兑换
- 当前订阅摘要
- 订阅列表摘要
- 最近订单列表摘要
- 最近兑换记录摘要

这部分目前是“摘要/列表视图”，不是单独的详情页路由系统。

### 3.4 帮助与安全

这部分用于承载当前版本说明、安全提醒和使用建议，强调：

- 不展示后端地址
- 不展示 API key
- 不展示 runtime token
- 官方模式可作为平台异常时的保底路径

## 4. 主要功能清单

### 4.1 已完成功能

- 亮色桌面应用壳
- 账户登录 / 注册 / 验证码 / 找回密码 / 重置密码
- 2FA 登录分支
- 用户信息同步
- 分组列表同步
- 订阅摘要同步
- 订单列表同步
- 兑换记录同步
- CDK 兑换
- Codex Desktop / CLI 安装检测
- 官方模式启动
- 平台代理 CLI 启动
- 受管 `CODEX_HOME` 生成
- desktop session 创建 / 刷新 / 回收
- Windows 安装包构建与 SHA256 输出

### 4.2 已明确但尚未完全闭环的功能

- 平台代理 Desktop：服务端契约和客户端入口存在，但 Windows Store 版桌面端已被产品层面显式拦截
- 订单/订阅详情页：当前只有摘要和列表，没有独立详情路由
- 充值：当前仍按公告页处理，不做真实支付闭环
- 代码签名：已有脚本预留，但当前安装包未签名

## 5. 启动模式设计

### 5.1 官方模式

官方模式用于保底：

- 不改官方配置
- 不注入平台代理环境
- 适合平台异常、服务端未升级或分组不满足条件时使用

### 5.2 平台代理模式

平台代理模式的基本流程是：

1. 用户登录客户端
2. 客户端读取可用 OpenAI 分组
3. 客户端调用 `/api/v1/desktop/sessions`
4. 服务端返回 `session_id / runtime_token / gateway_base_url / profile_key`
5. 客户端生成本次启动专属受管 home
6. 将模型提供方配置、runtime token、本次会话元数据写入受管目录
7. 启动 Codex 进程
8. 后台续期 desktop session
9. 进程退出后回收运行目录和会话

### 5.3 当前平台模式边界

- 只展示可用于 Codex 的 active OpenAI 分组
- 不再默认把 Anthropic 分组当成 Codex 启动目标
- Windows Store 版 Codex Desktop 不支持平台代理模式
- Windows 上的 `.cmd/.bat` wrapper 不再被误当成真实生命周期进程

## 6. 技术架构

### 6.1 桌面前端

- 技术栈：`Slint 1.16`
- 风格：亮色、桌面化、非网页感
- 主窗口定义：`desktop-client/ui/app-window.slint`
- 页面拆分：
  - `login.slint`
  - `forgot_password.slint`
  - `dashboard.slint`
  - `launch_panel.slint`
  - `redeem.slint`
  - `about.slint`

### 6.2 Rust 主程序

主程序入口在 `desktop-client/src/main.rs`，负责：

- UI 初始化
- 本地状态恢复
- 登录/注册/重置密码事件绑定
- 计费数据刷新
- 安装检测
- 官方启动
- 平台代理启动
- session 续期
- 退出回收

### 6.3 API 契约层

客户端当前已接入的主要后端接口包括：

- `/auth/login`
- `/auth/login/2fa`
- `/auth/register`
- `/auth/send-verify-code`
- `/auth/forgot-password`
- `/auth/reset-password`
- `/auth/refresh`
- `/auth/me`
- `/groups/available`
- `/redeem`
- `/redeem/history`
- `/subscriptions/summary`
- `/payment/orders/my`
- `/desktop/sessions`
- `/desktop/sessions/:id/refresh`
- `/desktop/sessions/:id`

### 6.4 平台集成层

平台层负责三个核心问题：

1. 检测本机是否安装 Codex Desktop / CLI
2. 生成受管 `CODEX_HOME`
3. 组装和启动不同目标的命令

当前关键实现点：

- WindowsApps 检测会优先识别真实 Desktop 入口，而不是包内 helper
- `.cmd/.bat/.ps1` 目标不再错误跟踪 wrapper 生命周期
- Windows Store Desktop 使用 `explorer.exe shell:AppsFolder\...` 启动官方模式
- Windows Store Desktop 的平台代理模式直接拦截

### 6.5 本地状态与凭据

客户端本地状态分两类：

- 普通状态：上次登录邮箱、runtime 元数据、临时目录
- 敏感状态：refresh token

其中：

- 普通状态走 `directories::ProjectDirs`
- refresh token 默认走系统凭据存储（`keyring`）
- 文件型凭据存储只作为测试替身，不是生产路径

## 7. 运行目录与数据路径

在 Windows 上，客户端主要使用以下路径：

- 应用状态目录：通常位于 `%LOCALAPPDATA%\\sub2api\\TokenClient`
- 启动日志：`%LOCALAPPDATA%\\sub2api\\TokenClient\\data\\logs\\startup.log`
- 运行时目录：`<state-root>\\runtime\\<session_id>\\<profile_key>`
- 受管 home 内容：
  - `config.toml`
  - `auth.json`
  - `runtime-session.json`

运行时目录是“本次平台会话专属”的，目的是：

- 不污染用户官方 `.codex`
- 每次启动隔离配置
- 可按 session 回收

## 8. 安全原则

客户端当前遵守以下安全原则：

1. 不在 UI 中展示后端 API 地址
2. 不在 UI 中展示 API key
3. 不在 UI 中展示 runtime token
4. 不把平台代理配置写入用户官方 `.codex`
5. refresh token 优先进入系统凭据管理
6. 运行目录按 session 拆分并自动回收

## 9. 构建、运行与发布

### 9.1 本地运行

推荐入口：

```powershell
.\start-desktop-client.vbs
```

调试入口：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

如果是在 VMware / Hyper-V 之类的虚拟机图形环境里启动，客户端会在检测到虚拟机 BIOS 指纹后优先强制 `SLINT_BACKEND=winit-software`，尽量避开虚拟显卡导致的窗口初始化闪退。

停止：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\stop-desktop-client.ps1
```

### 9.2 测试与检查

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
```

### 9.3 Windows 安装包

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "http://43.173.88.95:8080/"
```

当前安装包输出为：

- `dist\desktop-client\Sub2API-Desktop-Setup-0.1.0.exe`

打包方式：

- Inno Setup
- `PrivilegesRequired=lowest`
- 默认安装到 `%LOCALAPPDATA%\Programs\Sub2API Desktop Client`

## 10. 已验证结果

### 10.1 单元测试与构建

已验证：

- `61` 个库测试通过
- `2` 个主程序测试通过
- `cargo check` 通过
- Windows 安装包成功生成

### 10.2 服务端联调

已验证：

- 公开设置可访问
- 登录成功
- 分组列表可拉取
- desktop session create / refresh / delete 已打通
- CDK 兑换至少验证过一枚余额码成功入账

### 10.3 VMware Windows 11 测试

我在本机 `VMware Workstation Pro` 的一台 Win11 虚拟机上，做了较深的安装验证：

- 虚拟机类型：`win11-24h2-cn-auto`
- 已找到其自动安装应答文件和本地账户
- 通过离线注入脚本把安装介质和启动逻辑送进了系统盘

在该虚拟机内部，至少有一轮报告明确记录了以下成功项：

- `INSTALL_CODEX_CLI` 成功
- `CODEX_VERSION` 成功
- `Sub2API-Desktop-Setup-0.1.0.exe` 静默安装退出码 `0`
- `sub2api-desktop.exe` 被成功拉起，观测窗口内保持运行

但这轮 VM 验证仍有两个重要限制：

1. `WinRM / RDP` 启用失败
   - 原因是脚本以普通用户上下文执行，缺少管理员提升
2. GUI 最终前台截图没有稳定落到我们的客户端
   - 虚拟机桌面会恢复到先前的 Edge 会话，导致“前台窗口验证”不稳定

因此，当前可以认为：

- “安装成功”有证据
- “客户端可被拉起”有证据
- “后续可远程自动化接管”仍未最终闭环

另外，自当前版本起，客户端在检测到 `VMware` / `Virtual Machine` 一类 BIOS 指纹时，会在创建主窗口前自动切到 `Slint software renderer`，并把启动诊断写入 `%LOCALAPPDATA%\\sub2api\\TokenClient\\data\\logs\\startup.log`，用于排查“打开即闪退”的早期崩溃。

## 11. 当前已知边界

### 11.1 Windows Store Desktop 边界

Windows Store 版 Codex Desktop 当前只支持官方模式。

原因是：

- `shell:AppsFolder` 激活链路无法稳定吃到本次启动注入的独立 `CODEX_HOME`
- 所以 Windows Store Desktop 的平台代理模式已被产品层面显式拦截

### 11.2 平台代理模式边界

- 当前主打支持对象是 Codex CLI
- 只支持 active OpenAI 分组进入 Codex 相关平台模式
- Anthropic 分组不会再默认作为 Codex 启动目标

### 11.3 计费边界

- 已有摘要和列表
- 尚未做独立的订单详情 / 订阅详情页面路由
- 充值仍视作公告页，不做真实支付闭环

### 11.4 发布边界

- 当前安装包未签名
- 还没有最终“正式商用版”发布清单
- 生产 API 地址仍由打包参数决定

## 12. 建议的后续路线

### 12.1 近一步

优先建议继续完善：

1. 把虚拟机测试链升级成管理员级自举
2. 真正打通 WinRM 或 RDP
3. 在 VM 内完成 GUI 启动中心和 CLI 路径的完整回归

### 12.2 版本收口

建议下一阶段完成：

1. 签名安装包
2. 固化生产后端地址
3. 补充订单/订阅详情路由
4. 增加更清晰的“平台代理模式支持矩阵”

## 13. 关键文件

建议优先关注这些文件：

- `desktop-client/src/main.rs`
- `desktop-client/src/platform/install_detection.rs`
- `desktop-client/src/platform/launcher.rs`
- `desktop-client/src/platform/managed_home.rs`
- `desktop-client/src/app/launch_errors.rs`
- `desktop-client/ui/app-window.slint`
- `desktop-client/src/app/view_models/billing_vm.rs`
- `desktop-client/packaging/windows/desktop-client.iss`
- `build-desktop-installer.ps1`

## 14. 结尾判断

这版客户端已经不是“概念原型”，而是一套已经具备：

- 本地安装能力
- 账户闭环
- 服务端联调能力
- Codex 启动整合能力
- Windows 发布能力

的桌面客户端基础版本。

它当前最强的定位不是“正式商业版最终形态”，而是“已可持续迭代、可进入灰度测试、可继续向商用品质推进”的交付底座。
