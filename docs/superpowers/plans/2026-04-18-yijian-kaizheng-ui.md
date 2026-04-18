# 一键开整 UI Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `desktop-client` 从当前的亮色平铺式桌面壳升级为品牌名为 `一键开整` 的下钻式桌面产品，落地全新的登录入口、记住密码/免登录偏好、电子牛马状态文案、Remotion 品牌动态图标、全局更新弹窗壳和 Stitch 驱动的界面重构。

**Architecture:** 先在 Rust 侧补齐品牌常量、状态文案池、认证偏好持久化和结果态/下钻路由模型，再在 Slint 侧落共享品牌组件、登录壳、总览页、详情页、全局更新弹窗壳和结果态覆盖层。动态图标单独放到 `desktop-client/motion/` 的 Remotion 工作区中渲染为静态帧和序列帧，再由一个小型 `motion` 模块按状态轮播接入到开屏、登录成功过渡和关键状态位；完整的 Windows 更新检查、下载、校验和安装逻辑由独立子项目 `docs/superpowers/specs/2026-04-18-windows-desktop-update-design.md` 负责。

**Tech Stack:** Rust, Slint 1.16, reqwest, serde, directories, keyring, anyhow, npm, Remotion, Stitch

---

## Current Implementation State

- 已确认并提交的设计文档位于 `docs/superpowers/specs/2026-04-18-yijian-kaizheng-ui-design.md`。
- Windows 桌面更新系统的独立设计文档位于 `docs/superpowers/specs/2026-04-18-windows-desktop-update-design.md`。
- 当前窗口入口在 `desktop-client/ui/app-window.slint`，登录页在 `desktop-client/ui/screens/login.slint`，账户/启动/计费/帮助均以平铺大卡形式挂在同一个窗口里。
- 认证、安装检测、平台会话续期和计费数据拉取全部集中在 `desktop-client/src/main.rs` 中；目前没有“记住密码/免登录”偏好模型，也没有结果态覆盖层或品牌动效状态机。
- 本计划需要同步加入“系统级更新弹窗壳”和“手动检查更新入口”的 UI 接点，但不在本计划中实现完整的下载、校验、静默安装和重启逻辑。
- 现有测试主要是纯 Rust 单元测试和 UI 文案守卫测试，适合继续用“先写失败测试，再落 UI 状态模型，再跑 `cargo check` + 手工冒烟”的节奏推进。

## File Map

### Create

- `desktop-client/src/app/brand.rs`
- `desktop-client/src/app/motion.rs`
- `desktop-client/src/app/view_models/update_vm.rs`
- `desktop-client/ui/components/brand_panel.slint`
- `desktop-client/ui/components/result_overlay.slint`
- `desktop-client/ui/screens/overview.slint`
- `desktop-client/ui/screens/launch_detail.slint`
- `desktop-client/ui/screens/billing_detail.slint`
- `desktop-client/ui/screens/help_detail.slint`
- `desktop-client/ui/screens/update_dialog.slint`
- `desktop-client/motion/package.json`
- `desktop-client/motion/tsconfig.json`
- `desktop-client/motion/remotion.config.ts`
- `desktop-client/motion/src/index.ts`
- `desktop-client/motion/src/Root.tsx`
- `desktop-client/motion/src/compositions/BrandMark.tsx`
- `desktop-client/motion/src/compositions/StatusPulse.tsx`
- `desktop-client/motion/scripts/render-brand-assets.mjs`
- `docs/superpowers/design/2026-04-18-yijian-kaizheng-stitch-handoff.md`

### Modify

- `desktop-client/build.rs`
- `desktop-client/src/main.rs`
- `desktop-client/src/lib.rs`
- `desktop-client/src/app/mod.rs`
- `desktop-client/src/app/router.rs`
- `desktop-client/src/app/auth_flow.rs`
- `desktop-client/src/app/view_models/auth_vm.rs`
- `desktop-client/src/app/view_models/dashboard_vm.rs`
- `desktop-client/src/app/view_models/launch_vm.rs`
- `desktop-client/src/app/view_models/billing_vm.rs`
- `desktop-client/src/app/view_models/mod.rs`
- `desktop-client/src/storage/app_state.rs`
- `desktop-client/ui/app-window.slint`
- `desktop-client/ui/screens/login.slint`
- `desktop-client/ui/screens/forgot_password.slint`
- `desktop-client/ui/screens/dashboard.slint`
- `desktop-client/ui/screens/launch_panel.slint`
- `desktop-client/ui/screens/redeem.slint`
- `desktop-client/ui/screens/about.slint`
- `desktop-client/ui/screens/help_detail.slint`
- `desktop-client/ui/screens/update_dialog.slint`
- `desktop-client/README.md`

### Test

- `desktop-client/src/app/brand.rs`
- `desktop-client/src/app/auth_flow.rs`
- `desktop-client/src/app/router.rs`
- `desktop-client/src/app/view_models/auth_vm.rs`
- `desktop-client/src/app/view_models/launch_vm.rs`
- `desktop-client/src/app/view_models/billing_vm.rs`
- `desktop-client/src/app/view_models/update_vm.rs`
- `desktop-client/src/storage/app_state.rs`
- `desktop-client/src/lib.rs`

## Task 1: Add Brand Copy And Auth Preference Foundations

**Files:**
- Create: `desktop-client/src/app/brand.rs`
- Modify: `desktop-client/src/app/mod.rs`
- Modify: `desktop-client/src/storage/app_state.rs`
- Test: `desktop-client/src/app/brand.rs`
- Test: `desktop-client/src/storage/app_state.rs`

- [ ] **Step 1: Write the failing tests for product copy and auth preferences**

```rust
// desktop-client/src/app/brand.rs
#[cfg(test)]
mod tests {
    use super::{product_name, login_title, status_copy, StatusCopyTone};

    #[test]
    fn product_copy_matches_approved_brand() {
        assert_eq!(product_name(), "一键开整");
        assert_eq!(login_title(), "欢迎王者归来");
        assert_eq!(
            status_copy(StatusCopyTone::BillingDue, 0),
            "给你的电子牛马喂点草料。"
        );
        assert_ne!(product_name(), "Sub2API Desktop");
    }
}
```

```rust
// desktop-client/src/storage/app_state.rs
#[test]
fn app_state_round_trips_auth_preferences_and_sanitizes_auto_login() {
    let dir = tempfile::tempdir().unwrap();
    let store = AppStateStore::new(dir.path().to_path_buf());

    let prefs = AuthPreferences {
        remember_password: false,
        auto_login: true,
    };
    store.save_auth_preferences(&prefs).unwrap();

    let loaded = store.load_auth_preferences().unwrap().unwrap();
    assert!(!loaded.remember_password);
    assert!(!loaded.auto_login);
}
```

- [ ] **Step 2: Run the Rust tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib brand
```

Expected: FAIL with errors such as `file not found for module brand`, `cannot find function product_name`, and `no method named save_auth_preferences`.

- [ ] **Step 3: Implement the brand module and persisted auth preferences**

```rust
// desktop-client/src/app/brand.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCopyTone {
    Launching,
    RedeemSuccess,
    BillingDue,
    Retry,
    Cleanup,
}

pub const fn product_name() -> &'static str {
    "一键开整"
}

pub const fn login_title() -> &'static str {
    "欢迎王者归来"
}

pub const fn login_button_text() -> &'static str {
    "登录"
}

pub fn status_copy(tone: StatusCopyTone, index: usize) -> &'static str {
    let pool = match tone {
        StatusCopyTone::Launching => &[
            "你的电子牛马已就位。",
            "电子牛马正在赶路。",
        ][..],
        StatusCopyTone::RedeemSuccess => &[
            "电子牛马获得了一次投喂。",
            "这次投喂到账，电子牛马干劲很足。",
        ][..],
        StatusCopyTone::BillingDue => &[
            "给你的电子牛马喂点草料。",
            "电子牛马今天快没力气了。",
        ][..],
        StatusCopyTone::Retry => &[
            "电子牛马刚刚打了个盹，正在重新探路。",
            "电子牛马没找到路，正在重新探路。",
        ][..],
        StatusCopyTone::Cleanup => &[
            "电子牛马已安全收工，原环境保持干净。",
            "今天这趟活已经收尾，环境也替你收干净了。",
        ][..],
    };
    pool[index % pool.len()]
}
```

```rust
// desktop-client/src/storage/app_state.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AuthPreferences {
    pub remember_password: bool,
    pub auto_login: bool,
}

impl Default for AuthPreferences {
    fn default() -> Self {
        Self {
            remember_password: true,
            auto_login: false,
        }
    }
}

impl AuthPreferences {
    pub fn sanitized(&self) -> Self {
        Self {
            remember_password: self.remember_password,
            auto_login: self.remember_password && self.auto_login,
        }
    }
}

pub fn save_auth_preferences(&self, prefs: &AuthPreferences) -> Result<()> {
    fs::create_dir_all(&self.root)?;
    fs::write(
        self.auth_preferences_path(),
        serde_json::to_vec_pretty(&prefs.sanitized())?,
    )?;
    Ok(())
}

pub fn load_auth_preferences(&self) -> Result<Option<AuthPreferences>> {
    let path = self.auth_preferences_path();
    if !path.exists() {
        return Ok(None);
    }
    let prefs: AuthPreferences = serde_json::from_slice(&fs::read(path)?)?;
    Ok(Some(prefs.sanitized()))
}
```

```rust
// desktop-client/src/app/mod.rs
pub mod auth_flow;
pub mod brand;
pub mod launch_errors;
pub mod router;
pub mod view_models;
```

- [ ] **Step 4: Run the tests again to verify they pass**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib brand
```

Expected: PASS with the new brand tests and updated `AppStateStore` tests green.

- [ ] **Step 5: Commit the foundation changes**

```bash
git add desktop-client/src/app/brand.rs desktop-client/src/app/mod.rs desktop-client/src/storage/app_state.rs
git commit -m "feat: add yijian kaizheng brand foundations"
```

## Task 2: Refactor Login Flow State And Remember/Auto-Login Semantics

**Files:**
- Modify: `desktop-client/src/app/auth_flow.rs`
- Modify: `desktop-client/src/app/view_models/auth_vm.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/src/storage/app_state.rs`
- Test: `desktop-client/src/app/auth_flow.rs`
- Test: `desktop-client/src/app/view_models/auth_vm.rs`

- [ ] **Step 1: Write the failing tests for the login surface state**

```rust
// desktop-client/src/app/view_models/auth_vm.rs
#[test]
fn login_surface_state_matches_approved_copy_and_toggle_rules() {
    let state = AuthViewModel::for_login(
        AuthPreferences {
            remember_password: true,
            auto_login: true,
        },
        false,
    );

    assert_eq!(state.title(), "欢迎王者归来");
    assert_eq!(state.primary_action_text(), "登录");
    assert_eq!(state.remember_password_label(), "记住密码");
    assert_eq!(state.auto_login_label(), "免登录");
    assert!(state.show_password_fields());
    assert!(!state.show_totp_field());
}

#[test]
fn auto_login_is_disabled_when_remember_password_is_off() {
    let state = AuthViewModel::for_login(
        AuthPreferences {
            remember_password: false,
            auto_login: true,
        },
        false,
    );

    assert!(!state.auto_login_enabled());
    assert!(!state.auto_login_checked());
}
```

```rust
// desktop-client/src/app/auth_flow.rs
#[test]
fn should_restore_session_requires_saved_token_and_auto_login() {
    let prefs = AuthPreferences {
        remember_password: true,
        auto_login: false,
    };
    assert!(!should_restore_session(&prefs, true));
    assert!(should_restore_session(
        &AuthPreferences {
            remember_password: true,
            auto_login: true,
        },
        true
    ));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib auth_vm
```

Expected: FAIL with missing `for_login`, `remember_password_label`, `auto_login_label`, and `should_restore_session`.

- [ ] **Step 3: Implement the login surface model and preference-aware restore logic**

```rust
// desktop-client/src/app/view_models/auth_vm.rs
use crate::app::brand::{login_button_text, login_title};
use crate::storage::app_state::AuthPreferences;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthViewModel {
    pub remember_password: bool,
    pub auto_login: bool,
    pub show_totp_field: bool,
}

impl AuthViewModel {
    pub fn for_login(prefs: AuthPreferences, show_totp_field: bool) -> Self {
        let prefs = prefs.sanitized();
        Self {
            remember_password: prefs.remember_password,
            auto_login: prefs.auto_login,
            show_totp_field,
        }
    }

    pub fn title(&self) -> &'static str {
        login_title()
    }

    pub fn primary_action_text(&self) -> &'static str {
        login_button_text()
    }

    pub fn remember_password_label(&self) -> &'static str {
        "记住密码"
    }

    pub fn auto_login_label(&self) -> &'static str {
        "免登录"
    }

    pub fn auto_login_enabled(&self) -> bool {
        self.remember_password
    }

    pub fn auto_login_checked(&self) -> bool {
        self.remember_password && self.auto_login
    }

    pub fn show_password_fields(&self) -> bool {
        true
    }

    pub fn show_totp_field(&self) -> bool {
        self.show_totp_field
    }
}
```

```rust
// desktop-client/src/app/auth_flow.rs
use crate::storage::app_state::AuthPreferences;

pub fn should_restore_session(prefs: &AuthPreferences, has_refresh_token: bool) -> bool {
    prefs.sanitized().auto_login && has_refresh_token
}
```

```rust
// desktop-client/src/main.rs
fn preload_local_state(app: &AppWindow, app_state: &AppStateStore) {
    if let Ok(Some(email)) = app_state.load_last_email() {
        app.set_email(SharedString::from(email));
    }
    let prefs = app_state
        .load_auth_preferences()
        .ok()
        .flatten()
        .unwrap_or_default();
    app.set_remember_password(prefs.remember_password);
    app.set_auto_login(prefs.auto_login);
}

fn persist_auth_preferences(
    app_state: &AppStateStore,
    token_store: &SystemCredentialStore,
    remember_password: bool,
    auto_login: bool,
) -> anyhow::Result<()> {
    let prefs = AuthPreferences {
        remember_password,
        auto_login,
    }
    .sanitized();
    if !prefs.remember_password {
        token_store.clear_refresh_token()?;
    }
    app_state.save_auth_preferences(&prefs)?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests and a compile check**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib auth_vm
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
```

Expected: PASS for the auth view model/auth flow tests, and `cargo check` succeeds with the new preference plumbing.

- [ ] **Step 5: Commit the login-state changes**

```bash
git add desktop-client/src/app/auth_flow.rs desktop-client/src/app/view_models/auth_vm.rs desktop-client/src/main.rs desktop-client/src/storage/app_state.rs
git commit -m "feat: add login preference semantics"
```

## Task 3: Create The Stitch Handoff And Redesign The Login Shell

**Files:**
- Create: `docs/superpowers/design/2026-04-18-yijian-kaizheng-stitch-handoff.md`
- Create: `desktop-client/ui/components/brand_panel.slint`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/ui/screens/login.slint`
- Modify: `desktop-client/ui/screens/forgot_password.slint`
- Test: `desktop-client/src/lib.rs`

- [ ] **Step 1: Write the failing UI copy guard test**

```rust
// desktop-client/src/lib.rs
#[test]
fn login_shell_copy_matches_approved_brand_language() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let login = std::fs::read_to_string(manifest_dir.join("ui/screens/login.slint")).unwrap();
    let app_window = std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();

    assert!(login.contains("欢迎王者归来"));
    assert!(login.contains("记住密码"));
    assert!(login.contains("免登录"));
    assert!(login.contains("text: \"登录\""));
    assert!(app_window.contains("一键开整"));
    assert!(!login.contains("主路径依旧极简"));
    assert!(!app_window.contains("Sub2API Desktop"));
}
```

- [ ] **Step 2: Run the UI copy guard to verify it fails**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib login_shell_copy_matches_approved_brand_language
```

Expected: FAIL because the existing Slint files still contain `Sub2API` branding and do not include the new login labels.

- [ ] **Step 3: Create the Stitch handoff and implement the new login shell**

```md
<!-- docs/superpowers/design/2026-04-18-yijian-kaizheng-stitch-handoff.md -->
# 一键开整 Stitch Handoff

## Screen 1: Login
- Product name: 一键开整
- Hero copy: 欢迎王者归来
- Primary action: 登录
- Secondary actions: 创建账户, 忘记密码
- Toggles: 记住密码, 免登录
- Layout: dark brand panel + light login card

## Screen 2: Login 2FA Expand
- Keep the same shell
- Expand the login card inline
- Preserve the toggles and primary action alignment

## Screen 3: Forgot Password
- Keep the dark brand panel
- Show a focused reset flow card
- Do not use long explanatory prose
```

```slint
// desktop-client/ui/components/brand_panel.slint
export component BrandPanel inherits Rectangle {
    in property <image> hero-image;
    in property <string> product-name: "一键开整";
    in property <string> status-copy: "你的电子牛马已就位。";

    background: #081524;
    border-radius: 28px;
    border-color: #173a57;
    border-width: 1px;

    Image {
        x: 28px;
        y: 28px;
        width: 132px;
        height: 132px;
        source: root.hero-image;
    }

    Text {
        x: 28px;
        y: 190px;
        text: root.product-name;
        color: #f4fbff;
        font-size: 34px;
        font-weight: 800;
    }

    Text {
        x: 28px;
        y: 236px;
        width: parent.width - 56px;
        text: root.status-copy;
        color: #9fc2dd;
        font-size: 14px;
        wrap: word-wrap;
    }
}
```

```slint
// desktop-client/ui/screens/login.slint
export component LoginScreen inherits Rectangle {
    in-out property <string> email;
    in-out property <string> password;
    in-out property <string> verification-code;
    in-out property <bool> remember-password: true;
    in-out property <bool> auto-login: false;
    in property <bool> show-totp-field: false;
    in property <string> status-text: "请输入邮箱和密码。";

    callback login-requested();
    callback register-requested();
    callback forgot-password-requested();

    Text { x: 24px; y: 20px; text: "欢迎王者归来"; color: #17324a; font-size: 30px; font-weight: 800; }
    LineEdit { x: 24px; y: 92px; width: parent.width - 48px; height: 44px; text <=> root.email; placeholder-text: "name@example.com"; }
    LineEdit { x: 24px; y: 154px; width: parent.width - 48px; height: 44px; text <=> root.password; placeholder-text: "输入密码"; input-type: InputType.password; }
    if root.show-totp-field: LineEdit { x: 24px; y: 216px; width: parent.width - 48px; height: 44px; text <=> root.verification-code; placeholder-text: "输入 2FA 验证码"; }
    Rectangle { x: 24px; y: root.show-totp-field ? 278px : 216px; width: 124px; height: 34px; TouchArea { clicked => { root.remember-password = !root.remember-password; } } }
    Rectangle { x: 164px; y: root.show-totp-field ? 278px : 216px; width: 108px; height: 34px; enabled: root.remember-password; TouchArea { clicked => { if (root.remember-password) { root.auto-login = !root.auto-login; } } } }
    Button { x: 24px; y: root.show-totp-field ? 332px : 270px; width: parent.width - 48px; height: 46px; text: "登录"; clicked => { root.login-requested(); } }
}
```

```slint
// desktop-client/ui/app-window.slint
import { BrandPanel } from "components/brand_panel.slint";

export component AppWindow inherits Window {
    in-out property <bool> remember-password: true;
    in-out property <bool> auto-login: false;
    in-out property <bool> show-login-totp: false;
    in-out property <image> brand-motion-image;
    in-out property <string> brand-status-copy: "你的电子牛马已就位。";

    BrandPanel {
        x: 24px;
        y: 24px;
        width: 340px;
        height: parent.height - 48px;
        hero-image: root.brand-motion-image;
        product-name: "一键开整";
        status-copy: root.brand-status-copy;
    }

    LoginScreen {
        remember-password <=> root.remember-password;
        auto-login <=> root.auto-login;
        show-totp-field: root.show-login-totp;
        forgot-password-requested => { root.current-section = 3; }
    }
}
```

- [ ] **Step 4: Run the tests, compile, and smoke-check the new login shell**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib login_shell_copy_matches_approved_brand_language
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- The UI copy test passes.
- `cargo check` succeeds.
- The desktop app shows `一键开整`, `欢迎王者归来`, `记住密码`, and `免登录`.

- [ ] **Step 5: Commit the Stitch handoff and login-shell rewrite**

```bash
git add docs/superpowers/design/2026-04-18-yijian-kaizheng-stitch-handoff.md desktop-client/ui/components/brand_panel.slint desktop-client/ui/app-window.slint desktop-client/ui/screens/login.slint desktop-client/ui/screens/forgot_password.slint desktop-client/src/lib.rs
git commit -m "feat: redesign login shell for yijian kaizheng"
```

## Task 4: Add The Drill-Down Shell And Result Overlay

**Files:**
- Create: `desktop-client/ui/components/result_overlay.slint`
- Create: `desktop-client/ui/screens/overview.slint`
- Modify: `desktop-client/src/app/router.rs`
- Modify: `desktop-client/src/app/view_models/dashboard_vm.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/ui/screens/dashboard.slint`
- Test: `desktop-client/src/app/router.rs`

- [ ] **Step 1: Write the failing routing test for the new information architecture**

```rust
// desktop-client/src/app/router.rs
#[cfg(test)]
mod tests {
    use super::{DetailPane, Route};

    #[test]
    fn route_titles_match_summary_detail_result_structure() {
        assert_eq!(Route::Overview.title(), "总览");
        assert_eq!(Route::Launch.title(), "启动中心");
        assert_eq!(Route::Billing.title(), "计费中心");
        assert_eq!(Route::Help.title(), "帮助与安全");
        assert_eq!(DetailPane::Launch.title(), "启动详情");
        assert_eq!(DetailPane::Billing.title(), "计费详情");
        assert_eq!(DetailPane::Help.title(), "帮助详情");
    }
}
```

- [ ] **Step 2: Run the router test to verify it fails**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib router
```

Expected: FAIL because `Route::Overview` and `DetailPane` do not exist yet.

- [ ] **Step 3: Implement the overview page, detail panes, and result overlay**

```rust
// desktop-client/src/app/router.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Overview,
    Launch,
    Billing,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailPane {
    None,
    Launch,
    Billing,
    Help,
}

impl DetailPane {
    pub const fn title(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Launch => "启动详情",
            Self::Billing => "计费详情",
            Self::Help => "帮助详情",
        }
    }
}
```

```slint
// desktop-client/ui/components/result_overlay.slint
export component ResultOverlay inherits Rectangle {
    in property <bool> visible: false;
    in property <string> title: "正在进入";
    in property <string> message: "你的电子牛马已就位。";

    background: visible ? #091523cc : transparent;
    if visible: Rectangle {
        x: (parent.width - 420px) / 2;
        y: (parent.height - 180px) / 2;
        width: 420px;
        height: 180px;
        background: #0d2032;
        border-radius: 24px;
        Text { text: root.title; color: #f4fbff; font-size: 28px; font-weight: 800; }
        Text { text: root.message; color: #a4c7de; font-size: 14px; wrap: word-wrap; }
    }
}
```

```slint
// desktop-client/ui/screens/overview.slint
export component OverviewScreen inherits Rectangle {
    in property <string> welcome-text: "准备开整";
    callback open-launch-detail-requested();
    callback open-billing-detail-requested();
    callback open-help-detail-requested();
    Text { x: 24px; y: 20px; text: root.welcome-text; color: #17324a; font-size: 28px; font-weight: 800; }
    Rectangle { x: 24px; y: 86px; width: 240px; height: 136px; TouchArea { clicked => { root.open-launch-detail-requested(); } } }
    Rectangle { x: 280px; y: 86px; width: 240px; height: 136px; TouchArea { clicked => { root.open-billing-detail-requested(); } } }
    Rectangle { x: 536px; y: 86px; width: 240px; height: 136px; TouchArea { clicked => { root.open-help-detail-requested(); } } }
}
```

```slint
// desktop-client/ui/app-window.slint
in-out property <int> current-route: 0;
in-out property <int> current-detail-pane: 0;
in-out property <bool> result-overlay-visible: false;
in-out property <string> result-overlay-title: "正在进入";
in-out property <string> result-overlay-message: "你的电子牛马已就位。";

OverviewScreen {
    visible: root.current-route == 0;
    open-launch-detail-requested => { root.current-detail-pane = 1; }
    open-billing-detail-requested => { root.current-detail-pane = 2; }
    open-help-detail-requested => { root.current-detail-pane = 3; }
}

ResultOverlay {
    visible: root.result-overlay-visible;
    title: root.result-overlay-title;
    message: root.result-overlay-message;
}
```

- [ ] **Step 4: Run tests, compile, and manually confirm drill-down behavior**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib router
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- Router tests pass.
- `cargo check` succeeds.
- The app shows a summary-first overview, and clicking cards opens the matching detail pane instead of another flat section.

- [ ] **Step 5: Commit the drill-down shell**

```bash
git add desktop-client/src/app/router.rs desktop-client/src/app/view_models/dashboard_vm.rs desktop-client/src/main.rs desktop-client/ui/components/result_overlay.slint desktop-client/ui/screens/overview.slint desktop-client/ui/app-window.slint desktop-client/ui/screens/dashboard.slint
git commit -m "feat: add drill-down shell and result overlay"
```

## Task 5: Convert Launch, Billing, And Help Flows To Summary/Detail Surfaces

**Files:**
- Create: `desktop-client/ui/screens/launch_detail.slint`
- Create: `desktop-client/ui/screens/billing_detail.slint`
- Create: `desktop-client/ui/screens/help_detail.slint`
- Modify: `desktop-client/src/app/brand.rs`
- Modify: `desktop-client/src/app/view_models/launch_vm.rs`
- Modify: `desktop-client/src/app/view_models/billing_vm.rs`
- Modify: `desktop-client/ui/screens/launch_panel.slint`
- Modify: `desktop-client/ui/screens/redeem.slint`
- Modify: `desktop-client/ui/screens/about.slint`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/src/main.rs`
- Test: `desktop-client/src/app/view_models/launch_vm.rs`
- Test: `desktop-client/src/app/view_models/billing_vm.rs`

- [ ] **Step 1: Write the failing view-model tests for summary/detail content**

```rust
// desktop-client/src/app/view_models/launch_vm.rs
#[test]
fn launch_view_model_exposes_summary_and_detail_copy() {
    let vm = LaunchViewModel::from_targets(&[]);
    assert_eq!(vm.summary_title, "启动中心");
    assert!(vm.official_mode_summary.contains("官方模式"));
    assert!(vm.platform_mode_summary.contains("平台代理模式"));
    assert_eq!(vm.primary_action_text, "查看启动详情");
}
```

```rust
// desktop-client/src/app/view_models/billing_vm.rs
#[test]
fn billing_view_model_exposes_summary_action_and_detail_groups() {
    let vm = BillingViewModel::empty();
    assert_eq!(vm.summary_title, "计费中心");
    assert_eq!(vm.primary_action_text, "兑换 CDK");
    assert!(vm.detail_sections.contains(&"订单详情".to_string()));
    assert!(vm.detail_sections.contains(&"兑换记录".to_string()));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib launch_vm
```

Expected: FAIL because the new summary/detail fields are not present.

- [ ] **Step 3: Implement the summary/detail view models and the new detail screens**

```rust
// desktop-client/src/app/view_models/launch_vm.rs
pub struct LaunchViewModel {
    pub desktop_available: bool,
    pub cli_available: bool,
    pub status_text: String,
    pub summary_title: String,
    pub official_mode_summary: String,
    pub platform_mode_summary: String,
    pub primary_action_text: String,
}

impl LaunchViewModel {
    pub fn empty() -> Self {
        Self {
            desktop_available: false,
            cli_available: false,
            status_text: "尚未检测 Codex 安装".to_string(),
            summary_title: "启动中心".to_string(),
            official_mode_summary: "官方模式不改你的官方配置。".to_string(),
            platform_mode_summary: "平台代理模式会创建独立受管环境。".to_string(),
            primary_action_text: "查看启动详情".to_string(),
        }
    }
}
```

```rust
// desktop-client/src/app/view_models/billing_vm.rs
pub struct BillingViewModel {
    pub summary_title: String,
    pub primary_action_text: String,
    pub subscription_summary_text: String,
    pub subscription_lines: Vec<String>,
    pub order_lines: Vec<String>,
    pub redeem_history_lines: Vec<String>,
    pub detail_sections: Vec<String>,
}

impl BillingViewModel {
    pub fn empty() -> Self {
        Self {
            summary_title: "计费中心".to_string(),
            primary_action_text: "兑换 CDK".to_string(),
            subscription_summary_text: "暂无订阅摘要".to_string(),
            subscription_lines: vec!["登录后可查看当前订阅额度和到期时间。".to_string()],
            order_lines: vec!["最近订单会显示在这里。".to_string()],
            redeem_history_lines: vec!["最近兑换记录会显示在这里。".to_string()],
            detail_sections: vec![
                "订阅详情".to_string(),
                "订单详情".to_string(),
                "兑换记录".to_string(),
                "充值说明".to_string(),
            ],
        }
    }
}
```

```slint
// desktop-client/ui/screens/launch_detail.slint
export component LaunchDetailScreen inherits Rectangle {
    in property <string> official-mode-summary;
    in property <string> platform-mode-summary;
    in property <string> status-text;
    Text { x: 24px; y: 20px; text: "启动详情"; color: #17324a; font-size: 26px; font-weight: 800; }
    Text { x: 24px; y: 70px; width: parent.width - 48px; text: root.official-mode-summary; wrap: word-wrap; }
    Text { x: 24px; y: 142px; width: parent.width - 48px; text: root.platform-mode-summary; wrap: word-wrap; }
    Text { x: 24px; y: 228px; width: parent.width - 48px; text: root.status-text; wrap: word-wrap; }
}
```

```slint
// desktop-client/ui/screens/billing_detail.slint
export component BillingDetailScreen inherits Rectangle {
    in property <string> subscription-summary-text;
    in property <[string]> subscription-lines;
    in property <[string]> order-lines;
    in property <[string]> history-lines;
    Text { x: 24px; y: 20px; text: "计费详情"; color: #17324a; font-size: 26px; font-weight: 800; }
    Text { x: 24px; y: 68px; width: parent.width - 48px; text: root.subscription-summary-text; wrap: word-wrap; }
    for line[index] in root.subscription-lines : Text { x: 24px; y: 114px + index * 18px; text: line; }
    for line[index] in root.order-lines : Text { x: 24px; y: 208px + index * 18px; text: line; }
    for line[index] in root.history-lines : Text { x: 24px; y: 302px + index * 18px; text: line; }
}
```

```slint
// desktop-client/ui/screens/help_detail.slint
export component HelpDetailScreen inherits Rectangle {
    in property <string> summary-text: "帮助与安全";
    // 安装诊断、模式限制、隐私边界拆成 detail rows
}
```

- [ ] **Step 4: Run tests, compile, and manually verify the converted flows**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib launch_vm
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib billing_vm
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- The `launch_vm` and `billing_vm` tests pass.
- The app shows summary cards first and opens dedicated detail panes for launch, billing, and help.

- [ ] **Step 5: Commit the summary/detail flow conversion**

```bash
git add desktop-client/src/app/brand.rs desktop-client/src/app/view_models/launch_vm.rs desktop-client/src/app/view_models/billing_vm.rs desktop-client/src/main.rs desktop-client/ui/screens/launch_panel.slint desktop-client/ui/screens/launch_detail.slint desktop-client/ui/screens/redeem.slint desktop-client/ui/screens/billing_detail.slint desktop-client/ui/screens/about.slint desktop-client/ui/screens/help_detail.slint desktop-client/ui/app-window.slint
git commit -m "feat: convert client flows to summary detail pages"
```

## Task 6: Scaffold The Remotion Workspace And Render Brand-Motion Assets

**Files:**
- Create: `desktop-client/motion/package.json`
- Create: `desktop-client/motion/tsconfig.json`
- Create: `desktop-client/motion/remotion.config.ts`
- Create: `desktop-client/motion/src/index.ts`
- Create: `desktop-client/motion/src/Root.tsx`
- Create: `desktop-client/motion/src/compositions/BrandMark.tsx`
- Create: `desktop-client/motion/src/compositions/StatusPulse.tsx`
- Create: `desktop-client/motion/scripts/render-brand-assets.mjs`

- [ ] **Step 1: Run the missing Remotion smoke render to verify the workspace does not exist yet**

Run:

```powershell
Push-Location desktop-client/motion
npx remotion still BrandMarkIdle --frame=12 --scale=0.25
Pop-Location
```

Expected: FAIL because `desktop-client/motion` and the Remotion project files are not created yet.

- [ ] **Step 2: Create the Remotion workspace files**

```json
// desktop-client/motion/package.json
{
  "name": "yijian-kaizheng-motion",
  "private": true,
  "scripts": {
    "studio": "remotion studio src/index.ts",
    "render:assets": "node scripts/render-brand-assets.mjs"
  },
  "dependencies": {
    "react": "19.1.0",
    "react-dom": "19.1.0",
    "remotion": "4.0.340"
  },
  "devDependencies": {
    "@types/react": "19.1.2",
    "@types/react-dom": "19.1.2",
    "typescript": "5.8.3"
  }
}
```

```tsx
// desktop-client/motion/src/compositions/BrandMark.tsx
import {AbsoluteFill, interpolate, spring, useCurrentFrame, useVideoConfig} from 'remotion';

export const BrandMark: React.FC<{phase: 'idle' | 'launch' | 'lock'}> = ({phase}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const progress = spring({frame, fps, config: {damping: 18, stiffness: 120}});
  const glow = interpolate(progress, [0, 1], [0.25, phase === 'launch' ? 1 : 0.6]);

  return (
    <AbsoluteFill style={{backgroundColor: '#081524', alignItems: 'center', justifyContent: 'center'}}>
      <div
        style={{
          width: 96,
          height: 96,
          borderRadius: 28,
          transform: `rotate(45deg) scale(${interpolate(progress, [0, 1], [0.86, 1])})`,
          background: 'linear-gradient(145deg, #ffe07f, #ff9c44 60%, #ff7346)',
          boxShadow: `0 0 60px rgba(255, 156, 74, ${glow})`,
        }}
      />
    </AbsoluteFill>
  );
};
```

```js
// desktop-client/motion/scripts/render-brand-assets.mjs
import {bundle} from '@remotion/bundler';
import {renderStill, selectComposition} from '@remotion/renderer';
import path from 'node:path';
import fs from 'node:fs/promises';

const root = path.resolve(process.cwd());
const entry = path.join(root, 'src/index.ts');
const outDir = path.resolve(root, '../assets/brand-motion');

await fs.mkdir(outDir, {recursive: true});
const bundleLocation = await bundle({entryPoint: entry});

for (const phase of ['idle', 'launch', 'lock']) {
  const composition = await selectComposition({
    serveUrl: bundleLocation,
    id: `BrandMark-${phase}`,
    inputProps: {phase},
  });
  for (let frame = 0; frame < 12; frame += 1) {
    await renderStill({
      composition,
      serveUrl: bundleLocation,
      output: path.join(outDir, `${phase}-${String(frame).padStart(2, '0')}.png`),
      frame,
      imageFormat: 'png',
      inputProps: {phase},
    });
  }
}
```

- [ ] **Step 3: Install dependencies and render the motion assets**

Run:

```powershell
Push-Location desktop-client/motion
npm install
npm run render:assets
npx remotion still BrandMark-launch --frame=6 --scale=0.25
Pop-Location
```

Expected:

- `npm install` succeeds.
- `desktop-client/assets/brand-motion/` now contains `idle-00.png` ... `lock-11.png`.
- The still render command writes a preview frame without errors.

- [ ] **Step 4: Smoke-check the generated files before wiring them into Slint**

Run:

```powershell
Get-ChildItem desktop-client/assets/brand-motion
```

Expected: PNG frames for `idle`, `launch`, and `lock` states are present.

- [ ] **Step 5: Commit the Remotion workspace**

```bash
git add desktop-client/motion desktop-client/assets/brand-motion
git commit -m "feat: add yijian kaizheng motion workspace"
```

## Task 7: Wire Motion Assets Into Slint, Update Packaging Metadata, And Verify End-To-End

**Files:**
- Create: `desktop-client/src/app/motion.rs`
- Modify: `desktop-client/build.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/ui/components/brand_panel.slint`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/README.md`
- Test: `desktop-client/src/app/motion.rs`
- Test: `desktop-client/src/lib.rs`

- [ ] **Step 1: Write the failing tests for motion frame selection and Windows branding**

```rust
// desktop-client/src/app/motion.rs
#[cfg(test)]
mod tests {
    use super::{MotionPhase, MotionSequence};

    #[test]
    fn motion_sequence_rotates_frames_within_each_phase() {
        let seq = MotionSequence::new(12);
        assert_eq!(seq.frame_index(MotionPhase::Idle, 0), 0);
        assert_eq!(seq.frame_index(MotionPhase::Idle, 13), 1);
        assert_eq!(seq.frame_index(MotionPhase::Launch, 23), 11);
    }
}
```

```rust
// desktop-client/src/lib.rs
#[test]
fn windows_resource_metadata_uses_yijian_kaizheng_branding() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let build_rs = std::fs::read_to_string(manifest_dir.join("build.rs")).unwrap();

    assert!(build_rs.contains("一键开整"));
    assert!(!build_rs.contains("Sub2API Desktop Client"));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib motion_sequence_rotates_frames_within_each_phase
```

Expected: FAIL because `motion.rs` does not exist and `build.rs` still uses the old brand metadata.

- [ ] **Step 3: Implement motion sequencing, brand metadata, and UI wiring**

```rust
// desktop-client/src/app/motion.rs
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPhase {
    Idle,
    Launch,
    Lock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionSequence {
    frame_count: usize,
}

impl MotionSequence {
    pub const fn new(frame_count: usize) -> Self {
        Self { frame_count }
    }

    pub fn frame_index(&self, _phase: MotionPhase, tick: usize) -> usize {
        tick % self.frame_count
    }

    pub fn frame_path(&self, root: &Path, phase: MotionPhase, tick: usize) -> PathBuf {
        let phase_name = match phase {
            MotionPhase::Idle => "idle",
            MotionPhase::Launch => "launch",
            MotionPhase::Lock => "lock",
        };
        root.join(format!("{phase_name}-{:02}.png", self.frame_index(phase, tick)))
    }
}
```

```rust
// desktop-client/build.rs
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "一键开整");
        res.set("FileDescription", "一键开整桌面客户端");
        res.set("CompanyName", "Sub2API");
        res.compile().expect("failed to compile windows resources");
    }
    slint_build::compile("ui/app-window.slint").expect("failed to compile slint ui");
}
```

```rust
// desktop-client/src/main.rs
let motion_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/brand-motion");
let motion_sequence = MotionSequence::new(12);
let startup_app = app.as_weak();
std::thread::spawn(move || {
    for tick in 0..12 {
        let frame = motion_sequence.frame_path(&motion_root, MotionPhase::Idle, tick);
        let _ = startup_app.upgrade_in_event_loop(move |app| {
            if let Ok(image) = slint::Image::load_from_path(frame) {
                app.set_brand_motion_image(image);
            }
        });
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
});
```

```md
<!-- desktop-client/README.md -->
## 本轮品牌更新

- 正式产品名：`一键开整`
- 启动口号和电子牛马状态文案按状态轮换
- 开屏、登录成功和关键状态位使用 `desktop-client/motion/` 输出的品牌动态图标帧
```

- [ ] **Step 4: Run the full verification suite and manual smoke check**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
Push-Location desktop-client/motion
npm run render:assets
Pop-Location
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- All Rust unit tests pass.
- `cargo check` succeeds.
- Remotion assets re-render cleanly.
- The app boots with the `一键开整` brand, shows the new login shell, rotates brand frames, and the summary/detail/result flows remain usable.

- [ ] **Step 5: Commit the integrated UI refresh**

```bash
git add desktop-client/build.rs desktop-client/src/app/motion.rs desktop-client/src/main.rs desktop-client/ui/components/brand_panel.slint desktop-client/ui/app-window.slint desktop-client/README.md
git commit -m "feat: wire yijian kaizheng motion into desktop client"
```

## Task 8: Add Global Update Dialog Shell And Manual Check Entry

**Files:**
- Create: `desktop-client/src/app/view_models/update_vm.rs`
- Create: `desktop-client/ui/screens/update_dialog.slint`
- Modify: `desktop-client/src/app/view_models/mod.rs`
- Modify: `desktop-client/src/lib.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/ui/screens/help_detail.slint`
- Test: `desktop-client/src/app/view_models/update_vm.rs`
- Test: `desktop-client/src/lib.rs`

- [ ] **Step 1: Write the failing tests for the updater UI shell**

```rust
// desktop-client/src/app/view_models/update_vm.rs
#[cfg(test)]
mod tests {
    use super::{UpdateDialogState, UpdateViewModel};

    #[test]
    fn update_view_model_matches_optional_and_required_copy() {
        let optional = UpdateViewModel::optional(
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            "发现新版本".to_string(),
            "修复若干问题".to_string(),
        );
        assert!(!optional.force_update);
        assert_eq!(optional.primary_action_text, "立即更新");
        assert_eq!(optional.secondary_action_text.as_deref(), Some("稍后"));

        let required = UpdateViewModel::required(
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            "发现新版本".to_string(),
            "当前版本已停止支持".to_string(),
        );
        assert!(required.force_update);
        assert_eq!(required.primary_action_text, "立即更新");
        assert_eq!(required.secondary_action_text, None);
        assert_eq!(required.state, UpdateDialogState::AvailableRequired);
    }
}
```

```rust
// desktop-client/src/lib.rs
#[test]
fn update_dialog_shell_uses_approved_copy_and_help_entry() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dialog =
        std::fs::read_to_string(manifest_dir.join("ui/screens/update_dialog.slint")).unwrap();
    let help =
        std::fs::read_to_string(manifest_dir.join("ui/screens/help_detail.slint")).unwrap();

    assert!(dialog.contains("发现新版本"));
    assert!(dialog.contains("立即更新"));
    assert!(dialog.contains("稍后"));
    assert!(help.contains("检查更新"));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib update_view_model_matches_optional_and_required_copy
```

Expected: FAIL because `update_vm.rs` and `update_dialog.slint` do not exist yet.

- [ ] **Step 3: Implement the update view model, global dialog shell, and manual entry**

```rust
// desktop-client/src/app/view_models/update_vm.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDialogState {
    Idle,
    Checking,
    AvailableOptional,
    AvailableRequired,
    Downloading,
    ReadyToInstall,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateViewModel {
    pub state: UpdateDialogState,
    pub current_version: String,
    pub latest_version: String,
    pub title: String,
    pub summary: String,
    pub force_update: bool,
    pub primary_action_text: String,
    pub secondary_action_text: Option<String>,
}

impl UpdateViewModel {
    pub fn optional(
        current_version: String,
        latest_version: String,
        title: String,
        summary: String,
    ) -> Self {
        Self {
            state: UpdateDialogState::AvailableOptional,
            current_version,
            latest_version,
            title,
            summary,
            force_update: false,
            primary_action_text: "立即更新".to_string(),
            secondary_action_text: Some("稍后".to_string()),
        }
    }

    pub fn required(
        current_version: String,
        latest_version: String,
        title: String,
        summary: String,
    ) -> Self {
        Self {
            state: UpdateDialogState::AvailableRequired,
            current_version,
            latest_version,
            title,
            summary,
            force_update: true,
            primary_action_text: "立即更新".to_string(),
            secondary_action_text: None,
        }
    }
}
```

```rust
// desktop-client/src/app/view_models/mod.rs
pub mod auth_vm;
pub mod billing_vm;
pub mod dashboard_vm;
pub mod launch_vm;
pub mod update_vm;
```

```slint
// desktop-client/ui/screens/update_dialog.slint
import { Button } from "std-widgets.slint";

export component UpdateDialog inherits Rectangle {
    in property <bool> visible: false;
    in property <bool> force-update: false;
    in property <string> latest-version: "v0.0.0";
    in property <string> current-version: "v0.0.0";
    in property <string> summary: "当前版本已是最新。";
    in property <string> primary-text: "立即更新";
    in property <string> secondary-text: "稍后";

    callback primary-requested();
    callback secondary-requested();
    callback close-requested();

    if root.visible: Rectangle {
        x: 48px;
        y: 48px;
        width: parent.width - 96px;
        height: parent.height - 96px;
        background: #ffffff;
        border-radius: 28px;
        border-color: #d8e3ee;
        border-width: 1px;

        Text { x: 28px; y: 24px; text: "发现新版本"; color: #17324a; font-size: 28px; font-weight: 800; }
        Text { x: 28px; y: 86px; text: root.latest-version; color: #2b59d0; font-size: 20px; font-weight: 700; }
        Text { x: 28px; y: 122px; text: "当前版本 " + root.current-version + "，新版本已可用。"; color: #5f7892; font-size: 14px; }
        Text { x: 28px; y: 170px; width: parent.width - 56px; text: root.summary; color: #17324a; font-size: 14px; wrap: word-wrap; }

        if !root.force-update: Button {
            x: parent.width - 236px;
            y: parent.height - 74px;
            width: 88px;
            height: 42px;
            text: root.secondary-text;
            clicked => { root.secondary-requested(); }
        }

        Button {
            x: parent.width - 136px;
            y: parent.height - 74px;
            width: 108px;
            height: 42px;
            text: root.primary-text;
            clicked => { root.primary-requested(); }
        }
    }
}
```

```slint
// desktop-client/ui/screens/help_detail.slint
import { Button } from "std-widgets.slint";

export component HelpDetailScreen inherits Rectangle {
    callback manual-update-check-requested();

    Text { x: 24px; y: 20px; text: "帮助与安全"; color: #17324a; font-size: 26px; font-weight: 800; }
    Button {
        x: 24px;
        y: 76px;
        width: 132px;
        height: 40px;
        text: "检查更新";
        clicked => { root.manual-update-check-requested(); }
    }
}
```

```slint
// desktop-client/ui/app-window.slint
import { UpdateDialog } from "screens/update_dialog.slint";

in-out property <bool> update-dialog-visible: false;
in-out property <bool> update-force: false;
in-out property <string> update-current-version: "v0.1.0";
in-out property <string> update-latest-version: "v0.1.0";
in-out property <string> update-summary: "当前版本已是最新。";

callback manual-update-check-requested();
callback update-primary-requested();
callback update-secondary-requested();

UpdateDialog {
    visible: root.update-dialog-visible;
    force-update: root.update-force;
    current-version: root.update-current-version;
    latest-version: root.update-latest-version;
    summary: root.update-summary;
    primary-requested => { root.update-primary-requested(); }
    secondary-requested => { root.update-secondary-requested(); }
}
```

```rust
// desktop-client/src/main.rs
let manual_update_app = app.as_weak();
app.on_manual_update_check_requested(move || {
    if let Some(app) = manual_update_app.upgrade() {
        app.set_update_dialog_visible(true);
        app.set_update_force(false);
        app.set_update_current_version("v0.1.0".into());
        app.set_update_latest_version("v0.2.0".into());
        app.set_update_summary("修复若干问题，并提供更稳定的一键开整体验。".into());
    }
});
```

- [ ] **Step 4: Run the tests, compile, and manually confirm the update shell**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib update_view_model_matches_optional_and_required_copy
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib update_dialog_shell_uses_approved_copy_and_help_entry
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- The new update view model tests pass.
- The copy guard confirms `发现新版本` and `检查更新` exist in the UI.
- `cargo check` succeeds.
- Clicking `检查更新` from the help detail surface opens the global update dialog shell.

- [ ] **Step 5: Commit the update-shell UI hooks**

```bash
git add desktop-client/src/app/view_models/update_vm.rs desktop-client/src/app/view_models/mod.rs desktop-client/src/lib.rs desktop-client/src/main.rs desktop-client/ui/app-window.slint desktop-client/ui/screens/help_detail.slint desktop-client/ui/screens/update_dialog.slint
git commit -m "feat: add global desktop update dialog shell"
```
