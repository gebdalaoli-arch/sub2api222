# Desktop Client Token Billing Contract

- 日期：2026-04-19
- 面向分支：`codex/sub2api-desktop-client-v1`
- 目的：定义“桌面客户端只按 Token 充值、扣减和展示”的服务端契约，给后续服务端更新和桌面端接线提供唯一口径。

---

## 1. 目标边界

桌面客户端的费用体系以后只看 Token，不再看金额钱包。

服务端需要直接给桌面端返回：

- 剩余多少 Token
- 累计充值多少 Token
- 累计消费多少 Token

客户端不再做：

- 金额转 Token
- 根据渠道倍率临时换算余额
- 根据兑换记录再推导 Token

---

## 2. 设计原则

### 2.1 不改原版金额主账本

保留原版 `sub2api` 的：

- `users.balance`
- 金额充值/金额扣费
- 订阅额度逻辑

这些继续给原版 Web 和已有运营逻辑使用。

### 2.2 只新增客户端专用 Token 支线

桌面客户端依赖的是：

- `channels.settlement_unit = token`
- `client_token_wallets`
- `client_token_wallet_ledgers`
- `type=token` 的兑换码

### 2.3 Token 支线必须服务端闭环

服务端必须自行完成：

1. CDK 兑换加 Token
2. API Key 鉴权时检查 Token 钱包
3. usage 完成后扣 Token
4. 汇总接口直接返回 Token 结果

---

## 3. 数据库契约

## 3.1 channels

新增字段：

- `settlement_unit`
  - `money`
  - `token`
- `token_input_ratio_milli`
- `token_output_ratio_milli`
- `token_cache_write_ratio_milli`
- `token_cache_read_ratio_milli`

说明：

- 单位统一是 `milli-token / token`
- `1000` 表示“1 个原始 token 扣 1 个钱包 token”
- `2000` 表示“1 个原始 token 扣 2 个钱包 token”

## 3.2 client_token_wallets

字段：

- `user_id`
- `channel_id`
- `balance_milli_tokens`
- `total_recharged_milli_tokens`
- `total_consumed_milli_tokens`

语义：

- 一个用户在一个 Token 渠道下有一个钱包
- 当前版本按 `user_id + channel_id` 唯一

## 3.3 client_token_wallet_ledgers

字段：

- `user_id`
- `channel_id`
- `source_type`
- `source_id`
- `credit_milli_tokens`
- `debit_milli_tokens`
- `balance_after_milli_tokens`

当前使用的 `source_type`：

- `redeem_code`
- `usage`

---

## 4. 渠道结算契约

## 4.1 结算模式

桌面客户端渠道必须设置：

```json
{
  "settlement_unit": "token"
}
```

## 4.2 扣费公式

当前服务端使用：

```text
扣减 milli-token =
  input_tokens * token_input_ratio_milli +
  output_tokens * token_output_ratio_milli +
  cache_creation_tokens * token_cache_write_ratio_milli +
  cache_read_tokens * token_cache_read_ratio_milli
```

说明：

- 这里直接使用 usage 的原始 token 事实字段
- 不再经过金额换算
- 不依赖 `input_price_per_million_tokens`

---

## 5. CDK 契约

## 5.1 兑换码类型

新增：

- `type = token`

## 5.2 兑换码字段约束

当前实现中：

- `redeem_codes.value`
  - 直接存 Token 总数
  - 单位是 `token`
- `redeem_codes.group_id`
  - 必填
  - 用于定位该兑换码归属哪个渠道

## 5.3 兑换行为

当兑换 `type=token` 时：

1. 不改 `users.balance`
2. 根据 `group_id -> channel`
3. 给对应 `client_token_wallets` 加 Token
4. 写一条 `client_token_wallet_ledgers`

---

## 6. 请求前鉴权契约

对于非订阅分组：

- 如果分组所属渠道 `settlement_unit = money`
  - 继续走 `user.balance` 检查
- 如果分组所属渠道 `settlement_unit = token`
  - 改查 `client_token_wallets.balance_milli_tokens`

返回规则：

- 钱包有 Token：放行
- 钱包没有 Token：返回 `403`
  - code 仍保留 `INSUFFICIENT_BALANCE`
  - message 改成 `Insufficient token balance`

---

## 7. 请求后扣费契约

对于 unified usage billing：

- 金额渠道：
  - 原逻辑不变
- Token 渠道：
  - 不扣 `users.balance`
  - 改扣 `client_token_wallets`
  - 写 `client_token_wallet_ledgers`

当前仍保留：

- `actual_cost`
- API Key quota/rate-limit 更新
- account quota 更新

也就是说：

- Token 钱包负责桌面客户端结算
- 原有金额字段仍保留给原版站点和后台统计

---

## 8. 用户侧接口契约

## 8.1 Billing Summary

接口：

- `GET /api/v1/client/billing-summary`

返回：

```json
{
  "remaining_milli_tokens": 1234500,
  "recharged_milli_tokens": 2000000,
  "consumed_milli_tokens": 765500,
  "remaining_tokens": 1234.5,
  "recharged_tokens": 2000,
  "consumed_tokens": 765.5,
  "token_unit": "token"
}
```

## 8.2 Redeem History

接口：

- `GET /api/v1/redeem/history`

约束：

- `type=token` 的记录必须返回 `token_amount`

示例：

```json
{
  "code": "XXXX-XXXX",
  "type": "token",
  "value": 100000000,
  "token_amount": 100000000,
  "status": "used"
}
```

---

## 9. 桌面端后续接线建议

桌面客户端后续只需要接：

1. `GET /api/v1/client/billing-summary`
2. `GET /api/v1/redeem/history`
3. `GET /api/v1/usage`
4. `POST /api/v1/redeem`

优先级：

1. 首页余额改读 `billing-summary`
2. 计费中心改读 `billing-summary`
3. 兑换记录改读 `token_amount`
4. 消费明细继续读 `/usage` 的原始 token 字段

---

## 10. 升级红线

以后同步官方 `sub2api` 时，下面这些点不能被覆盖掉：

1. `channels.settlement_unit` 与四个 Token 倍率字段
2. `client_token_wallets`
3. `client_token_wallet_ledgers`
4. `type=token` 的兑换码逻辑
5. `/api/v1/client/billing-summary`
6. API Key 鉴权阶段对 Token 钱包的检查
7. unified usage billing 阶段对 Token 钱包的扣减

只要这七条还在，桌面客户端的 Token 结算链路就不会再退回“客户端自己算账”的旧状态。
