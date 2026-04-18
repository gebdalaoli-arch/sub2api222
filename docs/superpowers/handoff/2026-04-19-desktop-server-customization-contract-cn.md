# Desktop Client 服务端增量改造合同（相对官方 `sub2api`）

- 日期：2026-04-19
- 面向分支：`codex/sub2api-desktop-client-v1`
- 参考上游：`origin/main`（`Wei-Shaw/sub2api`）
- 目的：整理“为了支持自家 Windows 桌面客户端，我们在服务端相对官方 `sub2api` 做了哪些增量改造”，并明确后续跟进官方更新时必须保住的兼容契约，避免桌面客户端失效。

---

## 1. 这份文档解决什么问题

后续官方 `sub2api` 持续更新时，最容易出问题的不是“服务能不能启动”，而是：

1. 后端还能启动，但桌面客户端登录后无法启动平台代理模式。
2. 代理模式能启动，但问答完成后没有 usage / 扣费记录。
3. 客户端能检查更新，但拿不到安装包或公告瀑布。
4. 本地 Docker 更新时拉回了旧 `latest` 镜像，导致客户端依赖的接口又消失。

因此，后续所有跟官方同步代码的工作，都必须把下面几类**服务端契约**当成“不可随手覆盖的本地增量”。

---

## 2. 我们相对官方增加了哪些服务端能力

当前相对官方 `origin/main` 的服务端增量，核心只分四块：

1. `Desktop Session / Desktop Runtime` 能力
2. `Desktop Runtime usage / synthetic API key` 持久化能力
3. `Windows 桌面客户端更新与公告瀑布` 能力
4. `本地源码构建并更新 docker-compose.local.yml` 的部署能力

这四块里，前面三块直接决定桌面客户端是否可用；第四块决定部署时是否会把这些能力重新覆盖掉。

---

## 3. 必须长期保住的四个兼容契约

## 3.1 Desktop Session 契约

这是桌面客户端平台代理模式的基础。

如果缺失，客户端会出现：

- 登录成功但“平台代理模式”无法启动
- Desktop / CLI 无法拿到运行时凭据
- `CODEX_HOME` 受管配置没有对应网关

### 必须存在的接口

- `POST /api/v1/desktop/sessions`
- `POST /api/v1/desktop/sessions/:id/refresh`
- `DELETE /api/v1/desktop/sessions/:id`
- `POST /api/desktop/v1/responses`
- `POST /api/desktop/v1/chat/completions`
- `GET /api/desktop/v1/responses`

### 必须存在的数据库/模型能力

- `desktop_sessions` 表
- 桌面会话 Ent schema / repository / service / middleware / route wiring

### 关键文件组

- `backend/ent/schema/desktop_session.go`
- `backend/migrations/108_add_desktop_sessions.sql`
- `backend/migrations/109_add_desktop_session_group_context.sql`
- `backend/internal/repository/desktop_session_repo.go`
- `backend/internal/service/desktop_session.go`
- `backend/internal/server/middleware/desktop_runtime_auth.go`
- `backend/internal/handler/desktop_handler.go`
- `backend/internal/server/routes/desktop.go`

### 关键提交

- `7e9d88f feat: add desktop session domain model`
- `64818d7 feat: add desktop session http surface`
- `8d89593 fix: restore desktop runtime auth wiring`
- `a8d2b19 fix: enforce desktop session ownership and group context`
- `f06b821 fix: harden desktop session rollout coverage`

### 升级时的硬规则

- 不能删 `desktop_sessions` 表
- 不能改掉 `/api/v1/desktop/sessions*` 路由语义
- 不能把 `/api/desktop/v1/*` 改回只认普通 API key
- 不能去掉 group / ownership 校验

---

## 3.2 Desktop Runtime usage / 扣费契约

这是“平台代理模式问答完成后，usage 和扣费是否成立”的关键。

如果缺失，客户端表面看起来能工作，但会出现：

- 桌面 runtime 请求没有 `usage_logs.api_key_id`
- 管理后台和用户侧 usage 页面没有记录
- 扣费与用量追踪脱节

### 改造内容

我们为 Desktop Runtime 请求创建或复用持久化 synthetic API key，而不是只用内存态临时 key。

### 关键文件组

- `backend/internal/service/api_key_runtime.go`
- `backend/internal/repository/api_key_repo.go`
- `backend/internal/repository/api_key_repo_integration_test.go`

### 关键提交

- `9db9055 fix: persist desktop runtime usage keys`

### 升级时的硬规则

- 不能把 runtime 请求重新改回 `api_key_id = 0`
- 不能让 synthetic runtime key 出现在普通用户 API key 列表里
- 不能破坏 runtime 请求和 usage/billing 的关联

---

## 3.3 Windows 客户端更新 / 公告瀑布契约

这是新客户端“系统公告 + 客户端更新”功能依赖的服务端基础设施。

如果缺失，客户端会出现：

- 检查更新接口 404
- 能看到更新弹窗壳子，但拿不到真实版本信息
- 能看到版本信息，但下载不了安装包
- 公告瀑布没有内容或后台不可维护

### 必须存在的公开接口

- `GET /api/v1/desktop/updates/check`
- `GET /api/v1/desktop/updates/releases/:id`
- `GET /api/v1/desktop/updates/releases/:id/package`
- `GET /api/v1/desktop/updates/announcements`

### 必须存在的管理员接口

- `GET /api/v1/admin/desktop-updates/releases`
- `POST /api/v1/admin/desktop-updates/releases`
- `GET /api/v1/admin/desktop-updates/releases/:id`
- `PUT /api/v1/admin/desktop-updates/releases/:id`
- `DELETE /api/v1/admin/desktop-updates/releases/:id`
- `GET /api/v1/admin/desktop-updates/announcements`
- `POST /api/v1/admin/desktop-updates/announcements`
- `PUT /api/v1/admin/desktop-updates/announcements/:id`
- `DELETE /api/v1/admin/desktop-updates/announcements/:id`

### 必须存在的数据存储约定

- Setting key：`desktop_update_feed`
- 安装包落盘目录：`backend/data/desktop-updates/releases/...`
  - Docker 部署下，对应容器内 `/app/data/desktop-updates/releases/...`
  - 也就是说 `docker-compose.local.yml` 的 `./data:/app/data` 挂载不能丢

### 当前数据模型语义

桌面更新 feed 里至少包含：

- `releases`
- `standalone_announcements`
- `next_id`
- `next_announcement_id`

客户端可见的公告瀑布由两部分拼装：

1. 独立公告 `standalone_announcements`
2. 当前已发布版本里携带的 `announcement_items`

### 关键文件组

- `backend/internal/service/domain_constants.go`
- `backend/internal/service/desktop_update_models.go`
- `backend/internal/service/desktop_update_service.go`
- `backend/internal/handler/desktop_update_handler.go`
- `backend/internal/handler/admin/desktop_update_handler.go`
- `backend/internal/handler/dto/desktop_update.go`
- `backend/internal/server/routes/desktop.go`
- `backend/internal/server/routes/admin.go`
- `backend/internal/service/wire.go`

### 关键提交

- `69b5450 feat: add desktop update metadata service`
- `7671a28 feat: add desktop update public api`
- `abf29a6 feat: add admin desktop update endpoints`
- `6f79353 feat: add desktop update announcement waterfall`

### 升级时的硬规则

- 不能删除 `desktop_update_feed` 的读取/写入逻辑
- 不能把公开更新接口改成需要用户登录
- 不能把安装包下载改成客户端直连第三方对象存储
- 不能删掉 `standalone_announcements`
- 不能把 `/api/v1/desktop/updates/announcements` 改成只返回版本内公告

---

## 3.4 部署与镜像构建契约

这是“服务端功能明明在代码里，但部署后又没了”的防线。

当前最容易踩的坑是：

- 代码已经有桌面接口，但服务器仍在跑 Docker Hub 旧 `latest`
- `docker compose up -d` 后表面健康，但实际还是旧镜像

### 本地部署约定

我们当前推荐的是：

- `deploy/docker-compose.local.yml`
- `deploy/.env`
- 宿主机本地目录持久化 `deploy/data/`

### 新增脚本

- `deploy/update-local-docker-deployment.sh`

这个脚本的目的不是替代官方部署，而是确保：

1. 用**当前仓库源码**构建本地 `weishaw/sub2api:latest`
2. 再重启 `docker-compose.local.yml`
3. 避免把桌面相关增量又覆盖回官方旧镜像

### 关键提交/文档

- `65e4453 fix: increase docker frontend build memory`
- `5fe9dd1 docs: add desktop handoff and incremental update package`
- `deploy/update-local-docker-deployment.sh`
- `docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md`

### 升级时的硬规则

- 不要默认 `docker pull weishaw/sub2api:latest` 就当成完成升级
- 如果本地分支包含桌面增量，必须用仓库源码重建镜像
- `deploy/data/` 目录不能清空，否则客户端更新包和相关运行数据会丢失

---

## 4. 当前相对官方的服务端改造清单（按模块）

## 4.1 Backend 新增/修改模块

### A. Desktop Session / Runtime

- `backend/ent/schema/desktop_session.go`
- `backend/migrations/108_add_desktop_sessions.sql`
- `backend/migrations/109_add_desktop_session_group_context.sql`
- `backend/internal/repository/desktop_session_repo.go`
- `backend/internal/service/desktop_session.go`
- `backend/internal/handler/desktop_handler.go`
- `backend/internal/server/middleware/desktop_runtime_auth.go`
- `backend/internal/server/routes/desktop.go`
- `backend/internal/server/router.go`
- `backend/internal/service/wire.go`
- `backend/internal/handler/wire.go`
- `backend/internal/handler/handler.go`

### B. Runtime usage / synthetic API key

- `backend/internal/service/api_key_runtime.go`
- `backend/internal/repository/api_key_repo.go`

### C. Desktop update / announcement waterfall

- `backend/internal/service/desktop_update_models.go`
- `backend/internal/service/desktop_update_service.go`
- `backend/internal/handler/desktop_update_handler.go`
- `backend/internal/handler/admin/desktop_update_handler.go`
- `backend/internal/handler/dto/desktop_update.go`
- `backend/internal/server/routes/desktop.go`
- `backend/internal/server/routes/admin.go`
- `backend/internal/service/domain_constants.go`
- `backend/internal/service/wire.go`

### D. OAuth secret 注入

- `backend/internal/pkg/geminicli/constants.go`
- `backend/internal/pkg/geminicli/oauth.go`
- `backend/internal/pkg/antigravity/oauth.go`

这里的关键结论是：仓库不再内嵌第三方 client secret，运行环境必须通过环境变量提供。

---

## 4.2 Deploy / 运维脚本增量

- `deploy/update-local-docker-deployment.sh`
- `deploy/README.md`
- `docs/superpowers/deploy/2026-04-19-server-incremental-update-cn.md`

---

## 5. 后续同步官方更新时的推荐流程

## 5.1 不要直接做的事

以下操作非常危险：

1. 直接把官方 `origin/main` 强行覆盖到当前部署分支
2. 只更新 Docker 镜像，不重新检查桌面接口
3. 只看 `/health` 返回 `ok` 就认为客户端兼容没问题
4. 只验证网页端，不验证桌面链路

## 5.2 推荐升级流程

1. 先从官方同步代码到一个新分支
2. 用这份文档对照检查四个契约是否仍在
3. 如果官方改到了这些关键文件，必须手工合并，不要盲接
4. 重新跑下面的最小验证
5. 确认通过后，再构建镜像并更新服务器

---

## 6. 每次跟官方同步后必须执行的最小验证

## 6.1 后端单测

至少跑：

```bash
cd backend
go test ./internal/service -run 'TestDesktopUpdateService_(PublishAndCheckWindowsRelease|StandaloneAnnouncementsLifecycleAndWaterfall)' -v
go test ./internal/handler/admin -run 'TestDesktopUpdateAdminHandler_(CreateReleaseFromMultipartUpload|CreateStandaloneAnnouncement)' -v
go test ./internal/server/routes -run 'TestDesktopUpdateRoutes_(RegisterPublicCheckAndPackageEndpoints|PublicAnnouncementsEndpointResponds)' -v
go test ./internal/service -run TestDesktopSessionService_CreateRefreshRevoke -v
go test ./internal/server/middleware -run TestDesktopRuntimeAuthMiddleware_AcceptsRuntimeToken -v
```

## 6.2 公开接口冒烟

至少检查：

```bash
curl -i http://127.0.0.1:8080/health
curl -i "http://127.0.0.1:8080/api/v1/desktop/updates/check?platform=windows&arch=x64&current_version=0.1.0"
curl -i "http://127.0.0.1:8080/api/v1/desktop/updates/announcements?platform=windows&arch=x64"
```

预期：

- `/health` 返回 `200`
- `/desktop/updates/check` 不应该是 `404`
- `/desktop/updates/announcements` 不应该是 `404`

## 6.3 桌面客户端联动冒烟

至少确认：

1. 登录仍可成功
2. 平台 CLI 仍可创建 desktop session
3. 走平台代理问答后，`/api/v1/usage` 仍能看到记录
4. 客户端“检查更新”能拿到真实响应，而不是静态演示值

---

## 7. 回滚策略

如果升级官方后发现客户端不兼容，优先回滚以下内容：

1. 当前服务端镜像
2. `deploy/.env`
3. `deploy/docker-compose.local.yml`
4. `deploy/data/`（尤其是 `desktop-updates` 相关内容）

最重要的是：**即使回滚，也不要把 `desktop_update_feed` 和 `desktop_sessions` 相关结构回滚没了。**

---

## 8. 一句话版维护原则

后续不管官方 `sub2api` 怎么升级，只要还要让自家桌面客户端继续可用，就必须长期保住下面这三条：

1. **Desktop Session / Runtime 网关不能丢**
2. **Desktop Runtime usage 与 synthetic API key 不能回退**
3. **Desktop Updates / Announcement Waterfall 不能回退成官方空白状态**

如果这三条里任意一条被覆盖掉，客户端就会出现“能打开但不能真正工作”的退化。
