# Desktop Client Token Billing Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不破坏官方 `sub2api` 金额主账本的前提下，为桌面客户端落一条独立的 Token 结算支线，让客户端后续只展示服务端直接返回的 Token 汇总与 Token 明细。

**Architecture:** 保留原有 `users.balance`、订阅和金额扣费逻辑不动，只给“客户端渠道”增加 `token settlement` 配置、Token 钱包/流水、Token CDK 充值和 Token 汇总接口。运行时请求仍走现有 usage / gateway 链路，但在渠道为 `token` 时改为扣 Token 钱包并把快照写入 `usage_logs`。

**Tech Stack:** Go、Gin、Ent + 原生 SQL、PostgreSQL migration、Wire、Go test

---

## Scope

- 服务端先完成：
  - 客户端渠道 Token 结算配置
  - Token 钱包与流水
  - Token 类型 CDK 兑换
  - `/api/v1/client/billing-summary`
  - API Key 鉴权阶段的 Token 余额放行
  - usage billing 阶段的 Token 扣减
  - 桌面端兼容文档更新
- 本轮不做：
  - 原版 Web 支付入口的 Token 化
  - 管理后台前端页面联动
  - 桌面客户端界面改造

## File Map

- Modify: `backend/internal/domain/constants.go`
- Modify: `backend/internal/service/domain_constants.go`
- Modify: `backend/internal/service/channel.go`
- Modify: `backend/internal/service/channel_service.go`
- Modify: `backend/internal/repository/channel_repo.go`
- Modify: `backend/internal/handler/admin/channel_handler.go`
- Modify: `backend/internal/service/redeem_code.go`
- Modify: `backend/internal/service/redeem_service.go`
- Modify: `backend/internal/repository/redeem_code_repo.go`
- Modify: `backend/internal/service/usage_billing.go`
- Modify: `backend/internal/repository/usage_billing_repo.go`
- Modify: `backend/internal/service/usage_log.go`
- Modify: `backend/internal/repository/usage_log_repo.go`
- Modify: `backend/internal/server/middleware/api_key_auth.go`
- Modify: `backend/internal/server/middleware/api_key_auth_google.go`
- Modify: `backend/internal/server/routes/user.go`
- Modify: `backend/internal/handler/handler.go`
- Modify: `backend/internal/handler/wire.go`
- Modify: `backend/internal/service/wire.go`
- Modify: `backend/internal/repository/wire.go`
- Modify: `客户端壳子/docs/superpowers/handoff/2026-04-19-desktop-server-customization-contract-cn.md`
- Add: `backend/migrations/110_client_token_billing.sql`
- Add: `backend/internal/service/client_token_billing.go`
- Add: `backend/internal/repository/client_token_wallet_repo.go`
- Add: `backend/internal/handler/client_billing_handler.go`
- Add: `客户端壳子/docs/superpowers/handoff/2026-04-19-desktop-client-token-billing-contract-cn.md`

## Task 1: 落渠道 Token 结算配置

- [ ] 给 `channels` 表新增 `settlement_unit` 与四个 Token 倍率字段。
- [ ] 扩展 `service.Channel`、`ChannelService`、`channelRepository`、`admin/channel_handler` 的读写契约。
- [ ] 增加单测，锁定 `token` / `money` 两类渠道配置序列化和默认值行为。

## Task 2: 落 Token 钱包与汇总服务

- [ ] 新增 `client_token_wallets`、`client_token_wallet_ledgers` 两张表。
- [ ] 新增 `ClientTokenWalletRepository` 与 `ClientTokenBillingService`。
- [ ] 提供：
  - 汇总读取
  - Token 余额校验
  - CDK 充值记账
  - usage 扣减记账

## Task 3: 落 Token CDK 兑换

- [ ] 新增 `RedeemTypeToken` 常量。
- [ ] 复用 `redeem_codes.value` 作为 token 数量载体，`group_id` 作为目标渠道定位信息。
- [ ] 在 `RedeemService` 里新增 token 兑换分支，走 Token 钱包，不再改 `users.balance`。
- [ ] 扩展 DTO，让用户侧兑换历史能直接拿到 `token_amount`。

## Task 4: 落请求前放行与请求后扣减

- [ ] 在 API Key 鉴权中间件里增加 Token 渠道分支：
  - 渠道为 `token` 时不再检查 `user.balance`
  - 改查 Token 钱包余额
- [ ] 在 unified usage billing 中增加 Token 渠道路径：
  - 保留 API Key 配额/限速/账号配额逻辑
  - 跳过用户金额扣减
  - 改扣 Token 钱包
- [ ] 把 `actual_debit_milli_tokens` 快照写入 `usage_logs`

## Task 5: 落客户端汇总接口与文档

- [ ] 新增 `ClientBillingHandler`
- [ ] 注册 `/api/v1/client/billing-summary`
- [ ] 更新桌面服务端兼容文档
- [ ] 新增桌面 Token 计费契约文档

## Verification

- [ ] `go test ./internal/service ./internal/repository ./internal/server/middleware ./internal/handler/...`
- [ ] `go test ./internal/server/routes/...`
- [ ] `go test ./cmd/server/...`
- [ ] `go test ./...`（若时间允许）
- [ ] `go test ./internal/service -tags unit`
- [ ] `go test ./internal/repository -run Token -v`（新增针对性验证）
