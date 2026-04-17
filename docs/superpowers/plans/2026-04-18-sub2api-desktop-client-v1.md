# Sub2API Desktop Client V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个 `Slint + Rust` 的跨平台桌面客户端，并为 `sub2api` 增加最小桌面会话能力，使用户能在不暴露普通 API Key 的前提下启动本机已安装的 `Codex Desktop` 和 `Codex CLI`。

**Architecture:** 服务端新增 `/api/v1/desktop/*` 会话接口和 `/api/desktop/v1/*` 兼容网关入口，使用短期 `runtime_token` 代替普通 API Key。客户端新增 `desktop-client/` Rust 工程，负责账户认证、安装探测、官方模式启动、平台模式受管 `CODEX_HOME`、运行时注入和恢复清理。实现顺序按依赖从后端契约到客户端启动链路，再到账户页面和交付脚本推进。

**Tech Stack:** Go, Gin, SQL migrations, Wire, Rust, Slint, tokio, reqwest, serde, directories, keyring, anyhow, tracing

---

## Current Implementation State

- Backend Task 1 and Task 2 have been implemented and committed through `f06b821`.
- Desktop sessions now bind `group_id` at creation time and preserve group/subscription context during runtime auth.
- Refresh and revoke endpoints are owner-scoped by `session_id + user_id`.
- Migration compatibility is handled by keeping `108_add_desktop_sessions.sql` stable and adding `109_add_desktop_session_group_context.sql` for the new `group_id` context.
- The next implementation entry point is Task 3: bootstrap the `desktop-client/` Rust + Slint project.

## File Map

### Backend

- Create: `backend/ent/schema/desktop_session.go`
- Create: `backend/migrations/108_add_desktop_sessions.sql`
- Create: `backend/migrations/109_add_desktop_session_group_context.sql`
- Create: `backend/internal/service/desktop_session.go`
- Create: `backend/internal/service/desktop_session_test.go`
- Create: `backend/internal/server/middleware/desktop_runtime_auth.go`
- Create: `backend/internal/server/middleware/desktop_runtime_auth_test.go`
- Create: `backend/internal/handler/desktop_handler.go`
- Create: `backend/internal/server/routes/desktop.go`
- Modify: `backend/internal/handler/handler.go`
- Modify: `backend/internal/handler/wire.go`
- Modify: `backend/internal/service/wire.go`
- Modify: `backend/internal/server/router.go`
- Modify: `backend/internal/server/api_contract_test.go`

### Desktop Client

- Create: `desktop-client/Cargo.toml`
- Create: `desktop-client/build.rs`
- Create: `desktop-client/src/lib.rs`
- Create: `desktop-client/src/main.rs`
- Create: `desktop-client/src/app/mod.rs`
- Create: `desktop-client/src/app/router.rs`
- Create: `desktop-client/src/app/view_models/auth_vm.rs`
- Create: `desktop-client/src/app/view_models/dashboard_vm.rs`
- Create: `desktop-client/src/app/view_models/launch_vm.rs`
- Create: `desktop-client/src/api/http.rs`
- Create: `desktop-client/src/api/auth.rs`
- Create: `desktop-client/src/api/account.rs`
- Create: `desktop-client/src/api/desktop_sessions.rs`
- Create: `desktop-client/src/platform/install_detection.rs`
- Create: `desktop-client/src/platform/launcher.rs`
- Create: `desktop-client/src/platform/managed_home.rs`
- Create: `desktop-client/src/platform/runtime_session.rs`
- Create: `desktop-client/src/storage/app_state.rs`
- Create: `desktop-client/src/storage/secure_store.rs`
- Create: `desktop-client/ui/app-window.slint`
- Create: `desktop-client/ui/screens/login.slint`
- Create: `desktop-client/ui/screens/forgot_password.slint`
- Create: `desktop-client/ui/screens/dashboard.slint`
- Create: `desktop-client/ui/screens/launch_panel.slint`
- Create: `desktop-client/ui/screens/redeem.slint`
- Create: `desktop-client/ui/screens/about.slint`

### Tooling And Docs

- Create: `start-desktop-client.ps1`
- Create: `start-desktop-client.vbs`
- Create: `desktop-client/README.md`
- Modify: `README_CN.md`

## Task 1: Add The Desktop Session Domain Model

**Files:**
- Create: `backend/ent/schema/desktop_session.go`
- Create: `backend/migrations/108_add_desktop_sessions.sql`
- Create: `backend/migrations/109_add_desktop_session_group_context.sql`
- Create: `backend/internal/service/desktop_session.go`
- Test: `backend/internal/service/desktop_session_test.go`
- Modify: `backend/internal/service/wire.go`

- [ ] **Step 1: Write the failing desktop session service test**

```go
package service

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestDesktopSessionService_CreateRefreshRevoke(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(repo, func() time.Time { return now }, []byte("desktop-test-secret"))

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)
	require.NotEmpty(t, created.SessionID)
	require.NotEmpty(t, created.RuntimeToken)
	require.Equal(t, int64(42), created.UserID)

	refreshed, err := svc.Refresh(context.Background(), created.SessionID)
	require.NoError(t, err)
	require.True(t, refreshed.ExpiresAt.After(created.ExpiresAt))

	require.NoError(t, svc.Revoke(context.Background(), created.SessionID))
	stored := repo.mustGet(created.SessionID)
	require.NotNil(t, stored.RevokedAt)
}
```

- [ ] **Step 2: Run the service test to verify it fails**

Run: `cd backend && go test ./internal/service -run TestDesktopSessionService_CreateRefreshRevoke -v`

Expected: FAIL with errors such as `undefined: NewDesktopSessionService` and missing desktop session types.

- [ ] **Step 3: Add the schema and migration**

```go
// backend/ent/schema/desktop_session.go
package schema

import (
	"entgo.io/ent"
	"entgo.io/ent/schema/field"
)

type DesktopSession struct {
	ent.Schema
}

func (DesktopSession) Fields() []ent.Field {
	return []ent.Field{
		field.String("session_id").Unique(),
		field.Int64("user_id"),
		field.String("device_id"),
		field.String("device_name").Default(""),
		field.String("target"),
		field.String("status").Default("active"),
		field.String("runtime_token_hash"),
		field.String("profile_key"),
		field.Time("expires_at"),
		field.Time("last_seen_at"),
		field.Time("revoked_at").Optional().Nillable(),
	}
}
```

```sql
-- backend/migrations/108_add_desktop_sessions.sql
CREATE TABLE IF NOT EXISTS desktop_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    user_id BIGINT NOT NULL,
    device_id TEXT NOT NULL,
    device_name TEXT NOT NULL DEFAULT '',
    target TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    runtime_token_hash TEXT NOT NULL,
    profile_key TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_desktop_sessions_user_id ON desktop_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_desktop_sessions_device_id ON desktop_sessions(device_id);
CREATE INDEX IF NOT EXISTS idx_desktop_sessions_expires_at ON desktop_sessions(expires_at);
```

- [ ] **Step 4: Implement the service and Wire provider**

```go
// backend/internal/service/desktop_session.go
package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"time"

	"github.com/google/uuid"
)

type DesktopSessionTarget string

const (
	DesktopSessionTargetDesktop DesktopSessionTarget = "desktop"
	DesktopSessionTargetCLI     DesktopSessionTarget = "cli"
)

type DesktopSessionCreateRequest struct {
	UserID        int64
	DeviceID      string
	DeviceName    string
	Target        DesktopSessionTarget
	ClientVersion string
}

type DesktopSessionResult struct {
	SessionID      string
	UserID         int64
	RuntimeToken   string
	ProfileKey     string
	RefreshAfter   time.Duration
	ExpiresAt      time.Time
	GatewayBaseURL string
}

type DesktopSessionService struct {
	repo       DesktopSessionRepository
	now        func() time.Time
	signingKey []byte
}

func NewDesktopSessionService(repo DesktopSessionRepository, now func() time.Time, signingKey []byte) *DesktopSessionService {
	return &DesktopSessionService{repo: repo, now: now, signingKey: signingKey}
}

func (s *DesktopSessionService) Create(ctx context.Context, req DesktopSessionCreateRequest) (*DesktopSessionResult, error) {
	sessionID := uuid.NewString()
	token := uuid.NewString() + "." + uuid.NewString()
	expiresAt := s.now().Add(12 * time.Hour)
	record := &DesktopSession{
		SessionID:        sessionID,
		UserID:           req.UserID,
		DeviceID:         req.DeviceID,
		DeviceName:       req.DeviceName,
		Target:           string(req.Target),
		Status:           "active",
		RuntimeTokenHash: hashDesktopRuntimeToken(token),
		ProfileKey:       "platform-" + string(req.Target),
		ExpiresAt:        expiresAt,
		LastSeenAt:       s.now(),
	}
	if err := s.repo.Create(ctx, record); err != nil {
		return nil, err
	}
	return &DesktopSessionResult{
		SessionID:      sessionID,
		UserID:         req.UserID,
		RuntimeToken:   token,
		ProfileKey:     record.ProfileKey,
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      expiresAt,
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *DesktopSessionService) Refresh(ctx context.Context, sessionID string) (*DesktopSessionResult, error) {
	record, err := s.repo.GetBySessionID(ctx, sessionID)
	if err != nil {
		return nil, err
	}
	record.ExpiresAt = s.now().Add(12 * time.Hour)
	record.LastSeenAt = s.now()
	if err := s.repo.Update(ctx, record); err != nil {
		return nil, err
	}
	return &DesktopSessionResult{
		SessionID:      record.SessionID,
		UserID:         record.UserID,
		ProfileKey:     record.ProfileKey,
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      record.ExpiresAt,
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *DesktopSessionService) Revoke(ctx context.Context, sessionID string) error {
	return s.repo.Revoke(ctx, sessionID, s.now())
}

func (s *DesktopSessionService) ValidateRuntimeToken(ctx context.Context, token string) (*DesktopSession, error) {
	record, err := s.repo.GetByRuntimeTokenHash(ctx, hashDesktopRuntimeToken(token))
	if err != nil {
		return nil, err
	}
	if record.RevokedAt != nil || !record.ExpiresAt.After(s.now()) {
		return nil, ErrUnauthorized
	}
	return record, nil
}

func hashDesktopRuntimeToken(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}
```

```go
// backend/internal/service/wire.go
var ProviderSet = wire.NewSet(
	// existing providers...
	wire.Value(func() time.Time { return time.Now().UTC() }),
	NewDesktopSessionService,
)
```

- [ ] **Step 5: Run the service test again**

Run: `cd backend && go test ./internal/service -run TestDesktopSessionService_CreateRefreshRevoke -v`

Expected: PASS with `--- PASS: TestDesktopSessionService_CreateRefreshRevoke`.

- [ ] **Step 6: Commit**

```bash
git add backend/ent/schema/desktop_session.go backend/migrations/108_add_desktop_sessions.sql backend/internal/service/desktop_session.go backend/internal/service/desktop_session_test.go backend/internal/service/wire.go
git commit -m "feat: add desktop session domain model"
```

## Task 2: Expose Desktop Session APIs And Runtime Token Auth

**Files:**
- Create: `backend/internal/server/middleware/desktop_runtime_auth.go`
- Create: `backend/internal/server/middleware/desktop_runtime_auth_test.go`
- Create: `backend/internal/handler/desktop_handler.go`
- Create: `backend/internal/server/routes/desktop.go`
- Modify: `backend/internal/handler/handler.go`
- Modify: `backend/internal/handler/wire.go`
- Modify: `backend/internal/server/router.go`
- Modify: `backend/internal/server/api_contract_test.go`

- [ ] **Step 1: Write failing middleware and handler tests**

```go
package middleware

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

func TestDesktopRuntimeAuthMiddleware_AcceptsRuntimeToken(t *testing.T) {
	gin.SetMode(gin.TestMode)
	svc := &desktopRuntimeAuthServiceStub{}
	mw := NewDesktopRuntimeAuthMiddleware(svc)
	r := gin.New()
	r.GET("/api/desktop/v1/responses", gin.HandlerFunc(mw), func(c *gin.Context) {
		c.Status(http.StatusNoContent)
	})

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/v1/responses", nil)
	req.Header.Set("Authorization", "Bearer runtime-token-1")
	resp := httptest.NewRecorder()
	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
}
```

```go
package handler

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	middleware2 "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

func TestDesktopHandler_CreateSession(t *testing.T) {
	gin.SetMode(gin.TestMode)
	h := NewDesktopHandler(&desktopHandlerServiceStub{})
	r := gin.New()
	r.POST("/api/v1/desktop/sessions", func(c *gin.Context) {
		c.Set(string(middleware2.ContextKeyUser), middleware2.AuthSubject{UserID: 42})
		h.CreateSession(c)
	})

	req := httptest.NewRequest(http.MethodPost, "/api/v1/desktop/sessions", strings.NewReader(`{"target":"desktop","device_id":"d-1","device_name":"mbp","client_version":"0.1.0"}`))
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()
	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)
	require.Contains(t, resp.Body.String(), "\"session_id\"")
}
```

- [ ] **Step 2: Run the backend HTTP tests to verify they fail**

Run: `cd backend && go test ./internal/server/middleware ./internal/handler -run 'TestDesktop(RuntimeAuthMiddleware_AcceptsRuntimeToken|Handler_CreateSession)' -v`

Expected: FAIL because `NewDesktopRuntimeAuthMiddleware`, `NewDesktopHandler`, and desktop routes do not exist yet.

- [ ] **Step 3: Implement runtime-token auth middleware**

```go
// backend/internal/server/middleware/desktop_runtime_auth.go
package middleware

import (
	"strings"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type DesktopRuntimeAuthMiddleware gin.HandlerFunc

func NewDesktopRuntimeAuthMiddleware(svc *service.DesktopSessionService) DesktopRuntimeAuthMiddleware {
	return DesktopRuntimeAuthMiddleware(func(c *gin.Context) {
		authHeader := strings.TrimSpace(c.GetHeader("Authorization"))
		if authHeader == "" {
			AbortWithError(c, 401, "RUNTIME_TOKEN_REQUIRED", "Runtime token is required")
			return
		}
		token := strings.TrimSpace(strings.TrimPrefix(authHeader, "Bearer"))
		session, err := svc.ValidateRuntimeToken(c.Request.Context(), token)
		if err != nil {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}
		c.Set("desktop_session", session)
		c.Set(string(ContextKeyUser), AuthSubject{UserID: session.UserID})
		c.Next()
	})
}
```

- [ ] **Step 4: Implement the handler, route registration, and handler wiring**

```go
// backend/internal/handler/desktop_handler.go
package handler

import (
	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	middleware2 "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type DesktopHandler struct {
	service *service.DesktopSessionService
}

func NewDesktopHandler(service *service.DesktopSessionService) *DesktopHandler {
	return &DesktopHandler{service: service}
}

func (h *DesktopHandler) CreateSession(c *gin.Context) {
	subject, ok := middleware2.GetAuthSubjectFromContext(c)
	if !ok {
		response.Unauthorized(c, "User not authenticated")
		return
	}
	var req struct {
		Target        string `json:"target" binding:"required,oneof=desktop cli"`
		DeviceID      string `json:"device_id" binding:"required"`
		DeviceName    string `json:"device_name" binding:"required"`
		ClientVersion string `json:"client_version" binding:"required"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		response.BadRequest(c, "Invalid request: "+err.Error())
		return
	}
	result, err := h.service.Create(c.Request.Context(), service.DesktopSessionCreateRequest{
		UserID:        subject.UserID,
		DeviceID:      req.DeviceID,
		DeviceName:    req.DeviceName,
		Target:        service.DesktopSessionTarget(req.Target),
		ClientVersion: req.ClientVersion,
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, result)
}

func (h *DesktopHandler) RefreshSession(c *gin.Context) {
	result, err := h.service.Refresh(c.Request.Context(), c.Param("id"))
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, result)
}

func (h *DesktopHandler) DeleteSession(c *gin.Context) {
	if err := h.service.Revoke(c.Request.Context(), c.Param("id")); err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, gin.H{"message": "desktop session revoked"})
}
```

```go
// backend/internal/server/routes/desktop.go
package routes

import (
	"github.com/Wei-Shaw/sub2api/internal/handler"
	servermiddleware "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

func RegisterDesktopRoutes(
	r *gin.Engine,
	v1 *gin.RouterGroup,
	h *handler.Handlers,
	jwtAuth servermiddleware.JWTAuthMiddleware,
	desktopAuth servermiddleware.DesktopRuntimeAuthMiddleware,
	settingService *service.SettingService,
) {
	authenticated := v1.Group("/desktop")
	authenticated.Use(gin.HandlerFunc(jwtAuth))
	authenticated.Use(servermiddleware.BackendModeUserGuard(settingService))
	{
		authenticated.POST("/sessions", h.Desktop.CreateSession)
		authenticated.POST("/sessions/:id/refresh", h.Desktop.RefreshSession)
		authenticated.DELETE("/sessions/:id", h.Desktop.DeleteSession)
	}

	desktopGateway := r.Group("/api/desktop/v1")
	desktopGateway.Use(gin.HandlerFunc(desktopAuth))
	{
		desktopGateway.POST("/responses", h.OpenAIGateway.Responses)
		desktopGateway.POST("/chat/completions", h.OpenAIGateway.ChatCompletions)
		desktopGateway.GET("/responses", h.OpenAIGateway.ResponsesWebSocket)
	}
}
```

- [ ] **Step 5: Run the route and contract tests**

Run: `cd backend && go test ./internal/server/... ./internal/handler -run 'TestDesktop(RuntimeAuthMiddleware_AcceptsRuntimeToken|Handler_CreateSession)|TestAPIContract' -v`

Expected: PASS for the new tests, with the API contract test including `/api/v1/desktop/sessions`.

- [ ] **Step 6: Commit**

```bash
git add backend/internal/server/middleware/desktop_runtime_auth.go backend/internal/server/middleware/desktop_runtime_auth_test.go backend/internal/handler/desktop_handler.go backend/internal/server/routes/desktop.go backend/internal/handler/handler.go backend/internal/handler/wire.go backend/internal/server/router.go backend/internal/server/api_contract_test.go
git commit -m "feat: add desktop session http surface"
```

## Task 3: Bootstrap The Rust + Slint Desktop Client

**Files:**
- Create: `desktop-client/Cargo.toml`
- Create: `desktop-client/build.rs`
- Create: `desktop-client/src/lib.rs`
- Create: `desktop-client/src/main.rs`
- Create: `desktop-client/src/app/mod.rs`
- Create: `desktop-client/src/app/router.rs`
- Create: `desktop-client/ui/app-window.slint`
- Create: `desktop-client/ui/screens/about.slint`

- [ ] **Step 1: Create the crate with a failing smoke test**

```toml
# desktop-client/Cargo.toml
[package]
name = "sub2api-desktop"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
slint = "1.14"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
directories = "6"
keyring = "3"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env-filter"] }

[build-dependencies]
slint-build = "1.14"

[dev-dependencies]
tempfile = "3"
```

```rust
// desktop-client/src/lib.rs
#[cfg(test)]
mod tests {
    #[test]
    fn app_bootstrap_exposes_router_module() {
        let router_name = std::any::type_name::<crate::app::router::Route>();
        assert!(router_name.contains("Route"));
    }
}

pub mod app;
```

- [ ] **Step 2: Run the crate test to verify it fails**

Run: `cargo test --manifest-path desktop-client/Cargo.toml app_bootstrap_exposes_router_module -- --exact`

Expected: FAIL with `could not find app in crate root` or `could not find router`.

- [ ] **Step 3: Add the Slint build script, root window, and router skeleton**

```rust
// desktop-client/build.rs
fn main() {
    slint_build::compile("ui/app-window.slint").expect("failed to compile slint ui");
}
```

```rust
// desktop-client/src/app/router.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Login,
    ForgotPassword,
    Dashboard,
    Redeem,
    About,
}
```

```rust
// desktop-client/src/main.rs
slint::include_modules!();

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let app = AppWindow::new()?;
    app.run()?;
    Ok(())
}
```

```slint
// desktop-client/ui/app-window.slint
import { Button, VerticalBox } from "std-widgets.slint";

export component AppWindow inherits Window {
    width: 1240px;
    height: 860px;
    title: "Sub2API Desktop";

    VerticalBox {
        spacing: 16px;
        Text { text: "Sub2API Desktop"; font-size: 32px; }
        Button { text: "登录"; }
    }
}
```

```slint
// desktop-client/ui/screens/about.slint
import { AboutSlint, VerticalBox } from "std-widgets.slint";

export component AboutScreen inherits VerticalBox {
    AboutSlint { }
}
```

- [ ] **Step 4: Run compile and smoke tests**

Run: `cargo test --manifest-path desktop-client/Cargo.toml app_bootstrap_exposes_router_module -- --exact && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: PASS for the test and a successful `Finished dev [unoptimized + debuginfo]`.

- [ ] **Step 5: Commit**

```bash
git add desktop-client/Cargo.toml desktop-client/build.rs desktop-client/src/lib.rs desktop-client/src/main.rs desktop-client/src/app/mod.rs desktop-client/src/app/router.rs desktop-client/ui/app-window.slint desktop-client/ui/screens/about.slint
git commit -m "feat: bootstrap slint desktop client"
```

## Task 4: Implement Account Auth, Session Storage, And The Dashboard Shell

**Files:**
- Create: `desktop-client/src/api/http.rs`
- Create: `desktop-client/src/api/auth.rs`
- Create: `desktop-client/src/api/account.rs`
- Create: `desktop-client/src/storage/secure_store.rs`
- Create: `desktop-client/src/storage/app_state.rs`
- Create: `desktop-client/src/app/view_models/auth_vm.rs`
- Create: `desktop-client/src/app/view_models/dashboard_vm.rs`
- Create: `desktop-client/ui/screens/login.slint`
- Create: `desktop-client/ui/screens/forgot_password.slint`
- Create: `desktop-client/ui/screens/dashboard.slint`

- [ ] **Step 1: Write the failing auth persistence test**

```rust
#[cfg(test)]
mod tests {
    use super::AppStateStore;

    #[test]
    fn auth_state_round_trips_refresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = AppStateStore::new(dir.path().to_path_buf());
        store.save_refresh_token("refresh-token-123").unwrap();
        assert_eq!(store.load_refresh_token().unwrap().as_deref(), Some("refresh-token-123"));
    }
}
```

- [ ] **Step 2: Run the auth test to verify it fails**

Run: `cargo test --manifest-path desktop-client/Cargo.toml auth_state_round_trips_refresh_token -- --exact`

Expected: FAIL because `AppStateStore` and `save_refresh_token` do not exist yet.

- [ ] **Step 3: Implement the HTTP client, secure storage, and auth view model**

```rust
// desktop-client/src/api/http.rs
use reqwest::{Client, RequestBuilder};

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    access_token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder().build().expect("http client"),
            base_url: base_url.into(),
            access_token: None,
        }
    }

    pub fn with_access_token(mut self, access_token: Option<String>) -> Self {
        self.access_token = access_token;
        self
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let request = self.client.post(url);
        match &self.access_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}
```

```rust
// desktop-client/src/storage/app_state.rs
use anyhow::Result;
use std::{fs, path::PathBuf};

pub struct AppStateStore {
    root: PathBuf,
}

impl AppStateStore {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    pub fn save_refresh_token(&self, token: &str) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::write(self.root.join("refresh_token"), token)?;
        Ok(())
    }
    pub fn load_refresh_token(&self) -> Result<Option<String>> {
        let path = self.root.join("refresh_token");
        if !path.exists() { return Ok(None); }
        Ok(Some(fs::read_to_string(path)?))
    }
}
```

```rust
// desktop-client/src/app/view_models/auth_vm.rs
pub struct AuthViewModel {
    pub email: String,
    pub password: String,
    pub status_text: String,
}

impl AuthViewModel {
    pub fn new() -> Self {
        Self { email: String::new(), password: String::new(), status_text: String::new() }
    }
}
```

- [ ] **Step 4: Add the login and dashboard Slint screens**

```slint
// desktop-client/ui/screens/login.slint
import { Button, LineEdit, VerticalBox } from "std-widgets.slint";

export component LoginScreen inherits VerticalBox {
    in-out property <string> email;
    in-out property <string> password;
    VerticalBox {
        spacing: 12px;
        Text { text: "登录到 Sub2API"; font-size: 28px; }
        LineEdit { text <=> root.email; placeholder-text: "邮箱"; }
        LineEdit { text <=> root.password; placeholder-text: "密码"; }
        Button { text: "登录"; }
    }
}
```

```slint
// desktop-client/ui/screens/dashboard.slint
import { VerticalBox } from "std-widgets.slint";

export component DashboardScreen inherits VerticalBox {
    in property <string> balance_text;
    in property <string> usage_text;
    VerticalBox {
        spacing: 12px;
        Text { text: "仪表盘"; font-size: 30px; }
        Text { text: root.balance_text; }
        Text { text: root.usage_text; }
    }
}
```

- [ ] **Step 5: Run client tests and compile checks**

Run: `cargo test --manifest-path desktop-client/Cargo.toml auth_state_round_trips_refresh_token -- --exact && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: PASS for the persistence test and a clean cargo check.

- [ ] **Step 6: Commit**

```bash
git add desktop-client/src/api/http.rs desktop-client/src/api/auth.rs desktop-client/src/api/account.rs desktop-client/src/storage/secure_store.rs desktop-client/src/storage/app_state.rs desktop-client/src/app/view_models/auth_vm.rs desktop-client/src/app/view_models/dashboard_vm.rs desktop-client/ui/screens/login.slint desktop-client/ui/screens/forgot_password.slint desktop-client/ui/screens/dashboard.slint
git commit -m "feat: add desktop account auth shell"
```

## Task 5: Detect Installed Codex Targets And Launch Official Mode

**Files:**
- Create: `desktop-client/src/platform/install_detection.rs`
- Create: `desktop-client/src/platform/launcher.rs`
- Create: `desktop-client/src/app/view_models/launch_vm.rs`
- Create: `desktop-client/ui/screens/launch_panel.slint`

- [ ] **Step 1: Write the failing installation detection test**

```rust
#[cfg(test)]
mod tests {
    use super::{detect_targets_from_paths, LaunchTarget};

    #[test]
    fn detects_cli_and_desktop_from_known_windows_paths() {
        let targets = detect_targets_from_paths(&[
            r"C:\Users\tester\AppData\Roaming\npm\codex.cmd".into(),
            r"C:\Program Files\WindowsApps\OpenAI.Codex_26.409.7971.0_x64__2p2nqsd0c76g0\app\resources\codex.exe".into(),
        ]);
        assert!(targets.iter().any(|t| t.kind == LaunchTarget::Cli));
        assert!(targets.iter().any(|t| t.kind == LaunchTarget::Desktop));
    }
}
```

- [ ] **Step 2: Run the launch test to verify it fails**

Run: `cargo test --manifest-path desktop-client/Cargo.toml detects_cli_and_desktop_from_known_windows_paths -- --exact`

Expected: FAIL because `detect_targets_from_paths` and `LaunchTarget` do not exist.

- [ ] **Step 3: Implement install detection and official launcher**

```rust
// desktop-client/src/platform/install_detection.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTarget {
    Desktop,
    Cli,
}

#[derive(Debug, Clone)]
pub struct InstalledTarget {
    pub kind: LaunchTarget,
    pub executable: std::path::PathBuf,
}

pub fn detect_targets_from_paths(paths: &[std::path::PathBuf]) -> Vec<InstalledTarget> {
    let mut results = Vec::new();
    for path in paths {
        let text = path.to_string_lossy().to_lowercase();
        if text.ends_with("codex.cmd") || text.ends_with("codex.ps1") {
            results.push(InstalledTarget { kind: LaunchTarget::Cli, executable: path.clone() });
        } else if text.ends_with("codex.exe") && text.contains("windowsapps") {
            results.push(InstalledTarget { kind: LaunchTarget::Desktop, executable: path.clone() });
        }
    }
    results
}
```

```rust
// desktop-client/src/platform/launcher.rs
use anyhow::Result;
use std::process::Command;

use super::install_detection::InstalledTarget;

pub fn launch_official(target: &InstalledTarget) -> Result<()> {
    Command::new(&target.executable).spawn()?;
    Ok(())
}
```

- [ ] **Step 4: Add the launch panel UI**

```slint
// desktop-client/ui/screens/launch_panel.slint
import { Button, HorizontalBox, VerticalBox } from "std-widgets.slint";

export component LaunchPanel inherits VerticalBox {
    in property <bool> desktop_available;
    in property <bool> cli_available;
    HorizontalBox {
        spacing: 16px;
        Button { text: "官方模式启动 Desktop"; enabled: root.desktop_available; }
        Button { text: "官方模式启动 CLI"; enabled: root.cli_available; }
    }
}
```

- [ ] **Step 5: Run the target tests and cargo check**

Run: `cargo test --manifest-path desktop-client/Cargo.toml detects_cli_and_desktop_from_known_windows_paths -- --exact && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: PASS with the launch target test green.

- [ ] **Step 6: Commit**

```bash
git add desktop-client/src/platform/install_detection.rs desktop-client/src/platform/launcher.rs desktop-client/src/app/view_models/launch_vm.rs desktop-client/ui/screens/launch_panel.slint
git commit -m "feat: add official launch detection flow"
```

## Task 6: Implement Managed `CODEX_HOME` And Platform Mode Launch

**Files:**
- Create: `desktop-client/src/api/desktop_sessions.rs`
- Create: `desktop-client/src/platform/managed_home.rs`
- Create: `desktop-client/src/platform/runtime_session.rs`
- Modify: `desktop-client/src/platform/launcher.rs`
- Modify: `desktop-client/src/app/view_models/launch_vm.rs`

- [ ] **Step 1: Write the failing managed home test**

```rust
#[cfg(test)]
mod tests {
    use super::{ManagedHomePaths, write_platform_home};
    use tempfile::tempdir;

    #[test]
    fn platform_home_writes_config_and_auth_without_touching_official_home() {
        let temp = tempdir().unwrap();
        let paths = ManagedHomePaths::new(temp.path().to_path_buf(), "platform-desktop");
        write_platform_home(
            &paths,
            "http://127.0.0.1:8080/api/desktop/v1",
            "runtime-token-abc",
        ).unwrap();

        let config = std::fs::read_to_string(paths.codex_home.join("config.toml")).unwrap();
        let auth = std::fs::read_to_string(paths.codex_home.join("auth.json")).unwrap();
        assert!(config.contains("model_provider = \"OpenAI\""));
        assert!(config.contains("base_url = \"http://127.0.0.1:8080/api/desktop/v1\""));
        assert!(auth.contains("runtime-token-abc"));
    }
}
```

- [ ] **Step 2: Run the platform-mode test to verify it fails**

Run: `cargo test --manifest-path desktop-client/Cargo.toml platform_home_writes_config_and_auth_without_touching_official_home -- --exact`

Expected: FAIL because `ManagedHomePaths` and `write_platform_home` do not exist.

- [ ] **Step 3: Implement the desktop session API client and managed home writer**

```rust
// desktop-client/src/api/desktop_sessions.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DesktopSessionResponse {
    pub session_id: String,
    pub gateway_base_url: String,
    pub runtime_token: String,
    pub refresh_after: u64,
    pub profile_key: String,
}
```

```rust
// desktop-client/src/platform/managed_home.rs
use anyhow::Result;
use std::{fs, path::PathBuf};

pub struct ManagedHomePaths {
    pub root: PathBuf,
    pub codex_home: PathBuf,
}

impl ManagedHomePaths {
    pub fn new(root: PathBuf, profile_name: &str) -> Self {
        let codex_home = root.join(profile_name);
        Self { root, codex_home }
    }
}

pub fn write_platform_home(paths: &ManagedHomePaths, gateway_base_url: &str, runtime_token: &str) -> Result<()> {
    fs::create_dir_all(&paths.codex_home)?;
    fs::write(
        paths.codex_home.join("config.toml"),
        format!(
            "model_provider = \"OpenAI\"\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"{}\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n",
            gateway_base_url
        ),
    )?;
    fs::write(
        paths.codex_home.join("auth.json"),
        format!("{{\"OPENAI_API_KEY\":\"{}\"}}\n", runtime_token),
    )?;
    Ok(())
}
```

- [ ] **Step 4: Launch targets in platform mode by overriding `CODEX_HOME`**

```rust
// desktop-client/src/platform/launcher.rs
use anyhow::Result;
use std::process::Command;

use super::install_detection::InstalledTarget;

pub fn launch_platform(target: &InstalledTarget, codex_home: &std::path::Path) -> Result<()> {
    Command::new(&target.executable)
        .env("CODEX_HOME", codex_home)
        .spawn()?;
    Ok(())
}
```

```rust
// desktop-client/src/platform/runtime_session.rs
use anyhow::Result;
use tokio::time::{sleep, Duration};

use crate::api::desktop_sessions::DesktopSessionResponse;

pub async fn refresh_loop<F, Fut>(session: DesktopSessionResponse, mut refresh_fn: F) -> Result<()>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<DesktopSessionResponse>>,
{
    let mut current = session;
    loop {
        sleep(Duration::from_secs(current.refresh_after)).await;
        current = refresh_fn(&current.session_id).await?;
    }
}
```

- [ ] **Step 5: Run the managed-home test and cargo check**

Run: `cargo test --manifest-path desktop-client/Cargo.toml platform_home_writes_config_and_auth_without_touching_official_home -- --exact && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: PASS and the generated config using `CODEX_HOME` instead of mutating the official home.

- [ ] **Step 6: Commit**

```bash
git add desktop-client/src/api/desktop_sessions.rs desktop-client/src/platform/managed_home.rs desktop-client/src/platform/runtime_session.rs desktop-client/src/platform/launcher.rs desktop-client/src/app/view_models/launch_vm.rs
git commit -m "feat: add platform mode codex home isolation"
```

## Task 7: Finish The User-Facing Dashboard Pages

**Files:**
- Create: `desktop-client/ui/screens/redeem.slint`
- Modify: `desktop-client/ui/screens/dashboard.slint`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/src/api/account.rs`
- Modify: `desktop-client/src/app/view_models/dashboard_vm.rs`
- Modify: `desktop-client/src/app/view_models/auth_vm.rs`

- [ ] **Step 1: Write the failing redeem view model test**

```rust
#[cfg(test)]
mod tests {
    use super::DashboardViewModel;

    #[test]
    fn redeem_success_updates_status_message() {
        let mut vm = DashboardViewModel::new();
        vm.set_redeem_status("兑换成功");
        assert_eq!(vm.redeem_status, "兑换成功");
    }
}
```

- [ ] **Step 2: Run the dashboard test to verify it fails**

Run: `cargo test --manifest-path desktop-client/Cargo.toml redeem_success_updates_status_message -- --exact`

Expected: FAIL because `set_redeem_status` and `redeem_status` do not exist.

- [ ] **Step 3: Implement balance/usage/redeem account methods**

```rust
// desktop-client/src/api/account.rs
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DashboardSummary {
    pub balance_text: String,
    pub usage_text: String,
}

pub async fn redeem_code(client: &crate::api::http::ApiClient, code: &str) -> Result<()> {
    client
        .post("/api/v1/redeem")
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

```rust
// desktop-client/src/app/view_models/dashboard_vm.rs
pub struct DashboardViewModel {
    pub balance_text: String,
    pub usage_text: String,
    pub redeem_status: String,
}

impl DashboardViewModel {
    pub fn new() -> Self {
        Self {
            balance_text: "余额 --".into(),
            usage_text: "用量 --".into(),
            redeem_status: String::new(),
        }
    }

    pub fn set_redeem_status(&mut self, status: impl Into<String>) {
        self.redeem_status = status.into();
    }
}
```

- [ ] **Step 4: Add the redeem screen and wire it into the root window**

```slint
// desktop-client/ui/screens/redeem.slint
import { Button, LineEdit, VerticalBox } from "std-widgets.slint";

export component RedeemScreen inherits VerticalBox {
    in-out property <string> code_text;
    in property <string> status_text;
    VerticalBox {
        spacing: 12px;
        Text { text: "兑换 CDK"; font-size: 28px; }
        LineEdit { text <=> root.code_text; placeholder-text: "输入兑换码"; }
        Button { text: "立即兑换"; }
        Text { text: root.status_text; }
    }
}
```

```slint
// desktop-client/ui/app-window.slint
import { AboutScreen } from "./screens/about.slint";
import { DashboardScreen } from "./screens/dashboard.slint";
import { LaunchPanel } from "./screens/launch_panel.slint";
import { LoginScreen } from "./screens/login.slint";
import { RedeemScreen } from "./screens/redeem.slint";
```

- [ ] **Step 5: Run the client tests and compile checks**

Run: `cargo test --manifest-path desktop-client/Cargo.toml redeem_success_updates_status_message -- --exact && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: PASS with the dashboard view model and redeem screen compiling together.

- [ ] **Step 6: Commit**

```bash
git add desktop-client/ui/screens/redeem.slint desktop-client/ui/screens/dashboard.slint desktop-client/ui/app-window.slint desktop-client/src/api/account.rs desktop-client/src/app/view_models/dashboard_vm.rs desktop-client/src/app/view_models/auth_vm.rs
git commit -m "feat: add desktop account dashboard pages"
```

## Task 8: Add Run Scripts, Docs, And End-To-End Verification

**Files:**
- Create: `start-desktop-client.ps1`
- Create: `start-desktop-client.vbs`
- Create: `desktop-client/README.md`
- Modify: `README_CN.md`

- [ ] **Step 1: Write the failing smoke command checklist**

```text
1. cargo run --manifest-path desktop-client/Cargo.toml
2. go test ./internal/service ./internal/server/... -run Desktop -v
3. Verify official launch works with no `CODEX_HOME` override
4. Verify platform launch writes managed `config.toml` and `auth.json`
```

- [ ] **Step 2: Run the current full verification commands and capture failures**

Run: `cd backend && go test ./internal/service ./internal/server/... -run Desktop -v; cd ..; cargo check --manifest-path desktop-client/Cargo.toml`

Expected: initial failures until the earlier tasks are fully complete.

- [ ] **Step 3: Add one-click local run scripts**

```powershell
# start-desktop-client.ps1
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $repoRoot
cargo run --manifest-path desktop-client/Cargo.toml
```

```vbscript
' start-desktop-client.vbs
Set shell = CreateObject("WScript.Shell")
shell.Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -File ""D:\挣钱\提问\sub2api-official-src\start-desktop-client.ps1""", 0, False
```

````md
<!-- desktop-client/README.md -->
# Desktop Client

## Dev Run

```bash
cargo run --manifest-path desktop-client/Cargo.toml
```

## Verification

```bash
cd backend && go test ./internal/service ./internal/server/... -run Desktop -v
cargo test --manifest-path desktop-client/Cargo.toml
```
````

- [ ] **Step 4: Run the final verification suite**

Run: `cd backend && go test ./internal/service ./internal/server/... -run Desktop -v && cd .. && cargo test --manifest-path desktop-client/Cargo.toml && cargo check --manifest-path desktop-client/Cargo.toml`

Expected: all desktop-session backend tests pass, all Rust tests pass, and cargo check succeeds.

- [ ] **Step 5: Commit**

```bash
git add start-desktop-client.ps1 start-desktop-client.vbs desktop-client/README.md README_CN.md
git commit -m "chore: add desktop client run and verification docs"
```

## Self-Review

- Spec coverage: 后端桌面会话、桌面兼容网关、客户端骨架、账户认证、官方模式、平台模式、`CODEX_HOME` 隔离、CDK/余额/用量页面、运行脚本与验证均有对应任务。
- Placeholder scan: 本计划没有遗留占位词或“以后补充”的模糊表述。
- Type consistency: `DesktopSessionService`、`DesktopHandler`、`DesktopRuntimeAuthMiddleware`、`ManagedHomePaths`、`launch_platform`、`DashboardViewModel` 等名称在跨任务中保持一致。
