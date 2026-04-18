# Desktop Client Config And Billing Gap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐桌面客户端平台代理模式写入 `~/.codex/config.toml` 时缺失的关键字段，并把当前“计费中心”从摘要/兑换混合页升级为真实的计费详情页，同时记录源码里仍然存在的占位功能缺口。

**Architecture:** 保持现有 `Slint + Rust` 主程序结构不变，在 `managed_home.rs` 中扩展受管配置写入契约，在 `BillingViewModel + Slint screen + main.rs` 之间补上更完整的计费详情模型和绑定。对文档层面，同步更新 README/计划文档，让“当前边界”与实际实现一致。

**Tech Stack:** Rust、Slint、Serde/TOML、Cargo test/check

---

### Task 1: 补齐受管 `config.toml` 注入契约

**Files:**
- Modify: `客户端壳子/desktop-client/src/platform/managed_home.rs`
- Test: `客户端壳子/desktop-client/src/platform/managed_home.rs`（同文件单测）

- [ ] **Step 1: 先写失败测试，锁定缺失字段**

```rust
#[test]
fn inject_platform_config_writes_full_openai_contract() {
    let temp = tempdir().unwrap();
    let user_home = temp.path().join(".codex");
    std::fs::create_dir_all(&user_home).unwrap();
    std::fs::write(user_home.join("config.toml"), "model = \"legacy-model\"\n").unwrap();

    backup_user_codex_config(&user_home).unwrap();
    inject_platform_config_into_user_home(
        &user_home,
        "http://43.173.88.95:8080",
        "runtime-token-abc",
    )
    .unwrap();

    let injected = std::fs::read_to_string(user_home.join("config.toml")).unwrap();
    assert!(injected.contains("model_provider = \"OpenAI\""));
    assert!(injected.contains("model = \"gpt-5.4\""));
    assert!(injected.contains("review_model = \"gpt-5.4\""));
    assert!(injected.contains("model_reasoning_effort = \"xhigh\""));
    assert!(injected.contains("disable_response_storage = true"));
    assert!(injected.contains("network_access = \"enabled\""));
    assert!(injected.contains("windows_wsl_setup_acknowledged = true"));
    assert!(injected.contains("model_context_window = 1000000"));
    assert!(injected.contains("model_auto_compact_token_limit = 900000"));
    assert!(injected.contains("base_url = \"http://43.173.88.95:8080\""));
}
```

- [ ] **Step 2: 运行单测，确认当前实现确实失败**

Run: `cargo test --manifest-path 客户端壳子/desktop-client/Cargo.toml inject_platform_config_writes_full_openai_contract -- --exact`

Expected: FAIL，提示 `config.toml` 中缺少 `model` / `review_model` / `network_access` 等字段。

- [ ] **Step 3: 用最小实现补齐受管配置写入**

```rust
fn apply_managed_codex_contract(root: &mut toml::map::Map<String, TomlValue>, gateway_base_url: &str) {
    root.insert("model_provider".into(), TomlValue::String("OpenAI".into()));
    root.insert("model".into(), TomlValue::String("gpt-5.4".into()));
    root.insert("review_model".into(), TomlValue::String("gpt-5.4".into()));
    root.insert(
        "model_reasoning_effort".into(),
        TomlValue::String("xhigh".into()),
    );
    root.insert("disable_response_storage".into(), TomlValue::Boolean(true));
    root.insert("network_access".into(), TomlValue::String("enabled".into()));
    root.insert(
        "windows_wsl_setup_acknowledged".into(),
        TomlValue::Boolean(true),
    );
    root.insert("model_context_window".into(), TomlValue::Integer(1_000_000));
    root.insert(
        "model_auto_compact_token_limit".into(),
        TomlValue::Integer(900_000),
    );

    let providers = root
        .entry("model_providers")
        .or_insert_with(|| TomlValue::Table(Default::default()));
    let providers = providers.as_table_mut().expect("model_providers must be table");
    let openai = providers
        .entry("OpenAI")
        .or_insert_with(|| TomlValue::Table(Default::default()));
    let openai = openai.as_table_mut().expect("OpenAI provider must be table");
    openai.insert("name".into(), TomlValue::String("OpenAI".into()));
    openai.insert("base_url".into(), TomlValue::String(gateway_base_url.into()));
    openai.insert("wire_api".into(), TomlValue::String("responses".into()));
    openai.insert("requires_openai_auth".into(), TomlValue::Boolean(true));
}
```

- [ ] **Step 4: 同步修正隔离 runtime home 的写入内容**

```rust
write_platform_home(&paths, gateway_base_url, runtime_token)?;

let config = std::fs::read_to_string(paths.codex_home.join("config.toml"))?;
assert!(config.contains("review_model = \"gpt-5.4\""));
assert!(config.contains("network_access = \"enabled\""));
```

- [ ] **Step 5: 重新运行相关单测**

Run: `cargo test --manifest-path 客户端壳子/desktop-client/Cargo.toml managed_home -- --nocapture`

Expected: PASS，`managed_home` 相关测试全部通过。

- [ ] **Step 6: Commit**

```bash
git add 客户端壳子/desktop-client/src/platform/managed_home.rs
git commit -m "fix: complete managed codex config injection"
```

### Task 2: 把“计费中心”补成真实详情页

**Files:**
- Modify: `客户端壳子/desktop-client/src/app/view_models/billing_vm.rs`
- Modify: `客户端壳子/desktop-client/src/main.rs`
- Modify: `客户端壳子/desktop-client/ui/app-window.slint`
- Create: `客户端壳子/desktop-client/ui/screens/billing_detail.slint`
- Test: `客户端壳子/desktop-client/src/app/view_models/billing_vm.rs`

- [ ] **Step 1: 先写失败测试，描述详情页模型需要的字段**

```rust
#[test]
fn billing_view_model_exposes_balance_usage_and_detail_blocks() {
    let summary = SubscriptionSummary {
        active_count: 1,
        total_used_usd: 12.5,
        subscriptions: vec![SubscriptionSummaryItem {
            id: 1,
            group_id: 9,
            group_name: "OpenAI Pro".into(),
            status: "active".into(),
            daily_used_usd: 2.0,
            daily_limit_usd: 10.0,
            weekly_used_usd: 4.0,
            weekly_limit_usd: 30.0,
            monthly_used_usd: 12.5,
            monthly_limit_usd: 100.0,
            expires_at: Some("2025-01-02T15:04:05Z".into()),
        }],
    };

    let vm = BillingViewModel::from_summary_and_history(Some(&summary), &[], &[]);

    assert!(vm.balance_headline.contains("累计使用"));
    assert!(vm.subscription_detail_lines[0].contains("日"));
    assert!(vm.subscription_detail_lines[0].contains("周"));
    assert!(vm.order_detail_lines[0].contains("暂无"));
}
```

- [ ] **Step 2: 运行单测，确认现有模型不满足详情页需求**

Run: `cargo test --manifest-path 客户端壳子/desktop-client/Cargo.toml billing_view_model_exposes_balance_usage_and_detail_blocks -- --exact`

Expected: FAIL，当前 `BillingViewModel` 没有详情字段，或断言不成立。

- [ ] **Step 3: 扩展 `BillingViewModel`，拆出“总览卡片 + 订阅明细 + 订单明细 + 兑换记录”**

```rust
pub struct BillingViewModel {
    pub plan_title: String,
    pub balance_headline: String,
    pub usage_caption: String,
    pub subscription_summary_text: String,
    pub subscription_lines: Vec<String>,
    pub subscription_detail_lines: Vec<String>,
    pub order_lines: Vec<String>,
    pub order_detail_lines: Vec<String>,
    pub redeem_history_lines: Vec<String>,
}
```

- [ ] **Step 4: 新建 `billing_detail.slint`，替代当前把 `RedeemScreen` 当计费页的做法**

```slint
export component BillingDetailScreen inherits Rectangle {
    in property <string> plan-title;
    in property <string> balance-headline;
    in property <string> usage-caption;
    in property <string> subscription-summary-text;
    in property <[string]> subscription-lines;
    in property <[string]> subscription-detail-lines;
    in property <[string]> order-detail-lines;
    in property <[string]> history-lines;
    in-out property <string> redeem-code;
    in property <string> redeem-status-text;

    callback redeem-requested();
}
```

- [ ] **Step 5: 在 `app-window.slint` 中挂上新屏幕，并保留 CDK 兑换入口**

```slint
if root.current-section == 2: Rectangle {
    BillingDetailScreen {
        plan-title: root.billing-plan-title;
        balance-headline: root.billing-balance-headline;
        usage-caption: root.billing-usage-caption;
        subscription-summary-text: root.subscription-summary-text;
        subscription-lines: root.subscription-lines;
        subscription-detail-lines: root.subscription-detail-lines;
        order-detail-lines: root.order-detail-lines;
        history-lines: root.redeem-history-lines;
        redeem-code <=> root.redeem-code;
        redeem-status-text: root.redeem-status-text;
        redeem-requested => { root.redeem-requested(); }
    }
}
```

- [ ] **Step 6: 在 `main.rs` 中补充新属性绑定**

```rust
fn apply_billing_state(app: &AppWindow, billing: &BillingViewModel) {
    app.set_billing_plan_title(SharedString::from(billing.plan_title.clone()));
    app.set_billing_balance_headline(SharedString::from(billing.balance_headline.clone()));
    app.set_billing_usage_caption(SharedString::from(billing.usage_caption.clone()));
    app.set_subscription_summary_text(SharedString::from(
        billing.subscription_summary_text.clone(),
    ));
    app.set_subscription_lines(string_model(...));
    app.set_subscription_detail_lines(string_model(...));
    app.set_order_detail_lines(string_model(...));
    app.set_redeem_history_lines(string_model(...));
}
```

- [ ] **Step 7: 运行 ViewModel 和编译验证**

Run: `cargo test --manifest-path 客户端壳子/desktop-client/Cargo.toml billing_view_model -- --nocapture`

Expected: PASS

Run: `cargo check --manifest-path 客户端壳子/desktop-client/Cargo.toml`

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add 客户端壳子/desktop-client/src/app/view_models/billing_vm.rs 客户端壳子/desktop-client/src/main.rs 客户端壳子/desktop-client/ui/app-window.slint 客户端壳子/desktop-client/ui/screens/billing_detail.slint
git commit -m "feat: add desktop billing detail screen"
```

### Task 3: 把其他已知缺口写清楚并校正文档

**Files:**
- Modify: `客户端壳子/desktop-client/README.md`
- Modify: `docs/superpowers/plans/2026-04-19-desktop-client-config-and-billing-gap.md`

- [ ] **Step 1: 把源码里已确认的占位能力列出来**

```markdown
- “设置与帮助”当前仍是帮助页，不包含真正的设置面板
- 启动页文案写着“官方模式已收纳到高级设置”，但 UI 中没有可见入口
- `router.rs` 的枚举仍停留在旧导航模型，未覆盖公告中心等当前 section
```

- [ ] **Step 2: 更新 README 的“当前边界”**

```markdown
- 已有：受管 `config.toml` 完整契约、计费详情页（订阅/订单/兑换）
- 未接入：真正的设置页、官方模式高级入口、生产签名发布闭环
```

- [ ] **Step 3: 运行最终验证**

Run: `cargo test --manifest-path 客户端壳子/desktop-client/Cargo.toml --lib`

Expected: 通过或仅剩与本次改动无关的已知旧断言问题，并在结果里明确说明。

- [ ] **Step 4: Commit**

```bash
git add 客户端壳子/desktop-client/README.md docs/superpowers/plans/2026-04-19-desktop-client-config-and-billing-gap.md
git commit -m "docs: record desktop client delivery gaps"
```
