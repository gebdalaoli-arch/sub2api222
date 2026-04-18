# Sub2API Desktop Handoff And Incremental Update Design

- 日期：2026-04-19
- 状态：设计已确认，进入实施
- 范围：Desktop GUI 真实补测、客户端归档交接、服务器增量更新包
- 部署基线：自有服务器 `docker-compose.local.yml`

## 背景

当前分支已经完成了三类关键改动：

- Windows 桌面客户端在 VMware / Windows Store / 平台代理下的可启动性修复
- 服务端 Desktop Runtime 使用记录缺失修复
- Codex Desktop / CLI 的平台代理基础链路与真实扣费链路验证

但交付层面仍有三个缺口：

1. 虚拟机里尚未形成 `Codex Desktop GUI 内可见问答结果` 的完整证据闭环
2. 客户端还没有整理成可交接、可继续套壳的归档结构
3. 服务端两批更新尚未合并成面向现网的增量更新包和一键更新脚本

## 目标

- 在 VMware 虚拟机中补齐 `Codex Desktop` 的 GUI 可见问答证据
- 把桌面客户端当前能力、边界、关键入口和套壳接入点整理成可交接文档
- 将两批服务端更新合并为仅针对自有服务器的增量更新包
- 提供基于 `docker-compose.local.yml` 的一键更新脚本
- 形成一套可复核的交付目录：文档、脚本、测试记录、部署顺序

## 非目标

- 不做通用对外发布包
- 不支持 `systemd` 二进制安装路径的一键更新
- 不在本轮完成桌面客户端正式品牌外壳替换
- 不扩展到 macOS / Linux 自更新或跨平台发布
- 不修改现网部署形态为新的基础设施方案

## 已确认的决策

- 采用“方案 2：交付包式”
- 服务器部署基线按 `deploy/docker-compose.local.yml`
- 更新脚本优先服务自有现网，不做泛化多环境兼容
- 客户端交接以源码入口、能力边界、接线点为核心，而不是重新输出产品说明书
- 虚拟机补测需要把服务端扣费前后、窗口可见回复、会话/启动链路证据串起来

## 交付结构

本轮交付拆成三个主包，但统一沉淀到同一套文档结构下：

### 1. 虚拟机真实测试记录

输出内容：

- Desktop GUI 问答补测步骤
- 虚拟机环境说明
- 关键截图 / 文本证据索引
- 扣费与 usage 观测说明
- 已完成 / 未完成边界

### 2. 客户端归档交接包

输出内容：

- 当前客户端能力矩阵
- 关键代码入口和模块地图
- 平台启动链路说明
- VMware / Store / 平台代理特殊处理说明
- 后续套壳接入点
- 构建、测试、打包、验证入口

### 3. 服务器增量更新包

输出内容：

- 合并后的更新说明
- 受影响文件与配置项
- 面向 `docker-compose.local.yml` 的部署顺序
- 一键更新脚本
- 健康检查与回滚建议

## 技术方案

## A. Desktop GUI 真实补测

Desktop 当前已确认：

- 登录成功
- 平台 Desktop 会话可创建
- 平台 CLI 在虚拟机内可真实对话并扣费
- 服务端 Desktop Runtime 请求可返回结果并扣费

待补的是“GUI 可见回复”这一段。补测路径采用：

1. 使用虚拟机内登录好的用户启动平台代理 Desktop
2. 确认 `Codex Desktop` 主窗口出现且使用独立 `CODEX_HOME`
3. 在桌面 GUI 中发起一条可严格匹配的测试提示词
4. 捕获 GUI 中可见返回文本
5. 对照服务端余额 / usage 结果

证据优先级：

- 窗口截图 / UIAutomation 导出
- 虚拟机内生成的输出文件
- 服务端余额 / usage 接口对照

## B. 客户端归档交接

客户端交接文档不重复已有产品介绍，而是补“工程交接信息”：

- 从 [desktop-client/src/main.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/main.rs) 开始的事件接线
- [desktop-client/src/platform/launcher.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/launcher.rs) 的官方 / 平台启动差异
- [desktop-client/src/platform/runtime_bootstrap.rs](D:/挣钱/token/token客户端/客户端壳子/desktop-client/src/platform/runtime_bootstrap.rs) 的虚拟机兼容处理
- 受管 `CODEX_HOME` 的写入和清理位置
- Store Desktop 与 CLI 的探测边界
- 后续“外壳替换 / UI 套壳”应优先接触的界面和 ViewModel 层

归档目标是让后续接手人能快速回答：

- 现在能做什么
- 哪些地方改过
- 套壳该从哪里下手
- 哪些路径不要碰

## C. 服务器增量更新包

服务器更新包按“只服务当前现网”设计，基线为：

- `deploy/docker-compose.local.yml`
- `.env`
- 本地目录数据卷 `data/ postgres_data/ redis_data/`

合并的两批服务端更新包括：

1. Desktop Runtime 相关修复
   - Store Desktop 平台代理链路配套能力
   - 打包与平台代理相关联动说明
2. Usage 记录修复
   - 持久化 runtime synthetic API key
   - 隐藏系统 key，避免污染普通 API Key 列表

更新脚本行为应保持保守：

1. 备份关键文件
2. 校验部署目录
3. 同步本次增量文件
4. 执行 `docker compose -f docker-compose.local.yml up -d --build` 或拉镜像更新路径
5. 跑健康检查
6. 输出人工复核步骤

## 验收标准

本轮完成标准是：

- 虚拟机内能拿到 Desktop GUI 可见回复证据
- 交接文档能清楚说明客户端当前能力、关键入口、套壳接线点
- 服务端更新说明能覆盖部署顺序、验证顺序、回滚顺序
- 一键更新脚本可在 `docker-compose.local.yml` 基线下执行
- 最终文档明确哪些内容已验证，哪些仍是边界或后续项

## 风险与约束

- 虚拟机中的 Windows Store Desktop GUI 自动化可能受焦点、首启弹窗、窗口延迟影响
- 仅凭公网访问无法百分百证明现网宿主机目录结构，因此更新脚本会按仓库推荐的 `docker-compose.local.yml` 基线编写
- 如果现网实际不是该基线，脚本需要在部署前做一次目录适配

## 最终交付

本轮最终应至少落成：

- 一份 Desktop GUI 补测记录
- 一份客户端交接文档
- 一份服务器增量更新说明
- 一个一键更新脚本
- 对应的验证结果与提交记录

