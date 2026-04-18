# 客户端壳子

这个文件夹集中放置当前桌面客户端壳子的源码、脚本和相关文档，方便单独查看、启动和继续迭代。

## 目录结构

- [desktop-client](D:/挣钱/token/token客户端/客户端壳子/desktop-client)
  `Slint + Rust` 客户端源码与资源
- [docs](D:/挣钱/token/token客户端/客户端壳子/docs)
  客户端壳子相关说明、spec、plan、handoff 与验证文档
- [start-desktop-client.ps1](D:/挣钱/token/token客户端/客户端壳子/start-desktop-client.ps1)
  调试启动入口
- [start-desktop-client.vbs](D:/挣钱/token/token客户端/客户端壳子/start-desktop-client.vbs)
  隐藏窗口启动入口
- [stop-desktop-client.ps1](D:/挣钱/token/token客户端/客户端壳子/stop-desktop-client.ps1)
  停止当前客户端进程
- [build-desktop-installer.ps1](D:/挣钱/token/token客户端/客户端壳子/build-desktop-installer.ps1)
  构建 Windows 安装包
- [sign-desktop-installer.ps1](D:/挣钱/token/token客户端/客户端壳子/sign-desktop-installer.ps1)
  安装包签名入口

## 运行

推荐从这里直接启动：

```powershell
.\客户端壳子\start-desktop-client.vbs
```

如果要看 Cargo 输出：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\客户端壳子\start-desktop-client.ps1
```

停止客户端：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\客户端壳子\stop-desktop-client.ps1
```

