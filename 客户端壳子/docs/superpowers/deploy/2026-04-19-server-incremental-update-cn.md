# 服务器增量更新说明

- 日期：2026-04-19
- 面向环境：自有服务器 `docker-compose.local.yml` 部署
- 目标：合并两批服务端更新并提供可执行更新顺序

## 本次合并的更新

## 1. Desktop Runtime / 平台代理相关

本次客户端相关增量里，和服务端直接相关的部分是 Desktop Runtime 链路要与当前客户端能力对齐：

- 客户端会继续使用 `/api/v1/desktop/sessions` 及相关 refresh / revoke 接口
- 平台代理模式会继续依赖服务端桌面会话与 Desktop Runtime 网关
- 这部分要求线上服务端保留并正常运行桌面会话接口

## 2. Usage 记录修复

核心修复提交：`9db9055 fix: persist desktop runtime usage keys`

修复点：

- Desktop Runtime 请求不再使用只有内存态的临时 API key
- 为 runtime 请求创建或复用持久化 synthetic API key
- `usage_logs.api_key_id` 不再写成 `0`
- 隐藏系统 synthetic key，避免污染普通 API key 列表

修复结果：

- 扣费和 usage 记录能够同时成立
- 生效范围是所有走同一条 Desktop Runtime / 平台代理链路的用户
- 不是只对 `123@123.com` 生效

## 3. OAuth Secret 清理

核心提交：`fad9b1b fix: remove embedded oauth client secrets`

这一批会移除仓库内嵌的第三方 client secret，要求运行环境通过环境变量注入：

- `GEMINI_CLI_OAUTH_CLIENT_SECRET`
- `ANTIGRAVITY_OAUTH_CLIENT_SECRET`

## 部署基线

本说明假定你的服务器是：

- 用 [deploy/docker-compose.local.yml](D:/挣钱/token/token客户端/deploy/docker-compose.local.yml) 部署
- `.env` 位于 `deploy/.env`
- 数据目录位于：
  - `deploy/data`
  - `deploy/postgres_data`
  - `deploy/redis_data`

## 推荐部署顺序

## 1. 备份

至少备份：

- `deploy/.env`
- `deploy/docker-compose.local.yml`
- `deploy/config.yaml`（如果存在）
- `deploy/data/config.yaml`（如果存在）

## 2. 使用当前源码构建本地镜像

不要只拉取 Docker Hub 上旧的 `latest`，而是用当前代码构建同标签镜像：

```bash
docker build -f Dockerfile -t weishaw/sub2api:latest .
```

这样 `docker-compose.local.yml` 不需要改，仍会使用本机刚构建的同名镜像。

## 3. 重启部署

在 `deploy/` 目录中执行：

```bash
docker compose -f docker-compose.local.yml up -d
```

## 4. 健康检查

至少确认：

```bash
curl -fsS http://127.0.0.1:${SERVER_PORT:-8080}/health
```

返回：

```json
{"status":"ok"}
```

## 5. 人工复核

至少做以下冒烟测试：

1. 客户端登录是否正常
2. CLI 平台代理是否仍能真实问答
3. 扣费后 `/api/v1/usage` 是否不再为空
4. `/api/v1/usage/stats` 是否开始有值

## 一键更新脚本

配套脚本：

- [deploy/update-local-docker-deployment.sh](D:/挣钱/token/token客户端/deploy/update-local-docker-deployment.sh)

它会做：

1. 校验 Docker / Compose / 部署文件
2. 备份 `deploy/.env`、`deploy/docker-compose.local.yml`、配置文件
3. 用当前仓库源码构建本地 `weishaw/sub2api:latest`
4. 重启 `docker-compose.local.yml`
5. 执行健康检查
6. 输出下一步人工验证命令

## 回滚建议

如果更新后失败：

1. 先看日志：

```bash
cd deploy
docker compose -f docker-compose.local.yml logs --tail=200 sub2api
```

2. 若需要回滚，至少恢复：

- 备份目录中的 `.env`
- `docker-compose.local.yml`
- `config.yaml` / `data/config.yaml`

3. 如需回滚镜像，重新构建或重新标记到旧版本镜像，再执行：

```bash
cd deploy
docker compose -f docker-compose.local.yml up -d
```

## 部署后应告知业务侧的结论

部署完成后，可以对外说的最重要结论是：

- Desktop Runtime 的 usage 记录问题已按通用逻辑修复
- 所有走平台代理 / Desktop Runtime 链路的用户都会生效
- 不是只针对某个测试账号

