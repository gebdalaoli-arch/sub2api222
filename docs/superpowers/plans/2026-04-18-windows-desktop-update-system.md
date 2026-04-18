# Windows 桌面更新系统 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `desktop-client` 落地 Windows 首版自更新系统，包含客户端启动检查与手动检查、更新弹窗、下载进度、`sha256` 校验、静默安装与重启，以及自家后端的桌面更新接口和后台发布页。

**Architecture:** 后端新增一个独立的 `DesktopUpdateService`，用 `SettingRepository` 保存桌面更新元数据和公告瀑布，用 `backend/data/desktop-updates/` 保存上传的 Windows 安装包；管理后台新增 `DesktopUpdatesView`、对应 API 模块和路由；桌面客户端新增 `update` API、下载/校验/安装编排模块，以及与 UI 壳对接的更新状态机。现有面向 GitHub 的 `UpdateService` 和 `/admin/system/check-updates` 保持不动，桌面更新走新的 `/desktop/updates/*` 和 `/admin/desktop-updates/*`。

**Tech Stack:** Go, Gin, req/v3, Rust, Slint 1.16, reqwest, serde, Vue 3, TypeScript, Axios, Inno Setup

---

## Current Implementation State

- 更新系统设计文档位于 `docs/superpowers/specs/2026-04-18-windows-desktop-update-design.md`，UI 接点已同步到 `docs/superpowers/specs/2026-04-18-yijian-kaizheng-ui-design.md`。
- 后端当前有一个面向 GitHub 的 [update_service.go](D:/挣钱/token/token客户端/backend/internal/service/update_service.go) 和管理员系统路由 `/admin/system/check-updates`，这是服务端自身更新，不适合复用到桌面客户端更新链路。
- 后端已经有成熟的管理员公告管理结构：
  - [backend/internal/handler/admin/announcement_handler.go](D:/挣钱/token/token客户端/backend/internal/handler/admin/announcement_handler.go)
  - [frontend/src/views/admin/AnnouncementsView.vue](D:/挣钱/token/token客户端/frontend/src/views/admin/AnnouncementsView.vue)
  - 新的桌面更新后台页应尽量沿用这些列表、分页、编辑对话框和 `BaseDialog` 模式。
- 桌面客户端已有：
  - Windows 安装包脚本 [build-desktop-installer.ps1](D:/挣钱/token/token客户端/build-desktop-installer.ps1)
  - Inno Setup 脚本 [desktop-client.iss](D:/挣钱/token/token客户端/desktop-client/packaging/windows/desktop-client.iss)
  - 但尚无版本检查、下载、校验、安装编排代码。
- 本计划以 `Windows` 为唯一目标平台；macOS/Linux 明确不进入本轮实现。

## File Map

### Create

- `backend/internal/service/desktop_update_service.go`
- `backend/internal/service/desktop_update_service_test.go`
- `backend/internal/service/desktop_update_models.go`
- `backend/internal/handler/desktop_update_handler.go`
- `backend/internal/handler/admin/desktop_update_handler.go`
- `backend/internal/handler/admin/desktop_update_handler_test.go`
- `backend/internal/handler/dto/desktop_update.go`
- `backend/internal/server/routes/desktop_updates_test.go`
- `frontend/src/api/admin/desktopUpdates.ts`
- `frontend/src/views/admin/DesktopUpdatesView.vue`
- `desktop-client/src/api/update.rs`
- `desktop-client/src/platform/updater.rs`
- `desktop-client/src/platform/updater_test.rs`
- `desktop-client/src/app/view_models/update_vm.rs`
- `desktop-client/ui/screens/update_dialog.slint`

### Modify

- `backend/internal/service/setting.go`
- `backend/internal/service/setting_service.go`
- `backend/internal/handler/handler.go`
- `backend/internal/handler/wire.go`
- `backend/internal/server/routes/desktop.go`
- `backend/internal/server/routes/admin.go`
- `frontend/src/api/admin/index.ts`
- `frontend/src/router/index.ts`
- `frontend/src/components/layout/AppSidebar.vue`
- `frontend/src/types/index.ts`
- `frontend/src/i18n/locales/zh.ts`
- `frontend/src/i18n/locales/en.ts`
- `desktop-client/src/lib.rs`
- `desktop-client/src/main.rs`
- `desktop-client/src/app/mod.rs`
- `desktop-client/src/app/view_models/mod.rs`
- `desktop-client/src/storage/app_state.rs`
- `desktop-client/ui/app-window.slint`
- `desktop-client/ui/screens/help_detail.slint`
- `desktop-client/README.md`
- `desktop-client/packaging/windows/desktop-client.iss`

### Test

- `backend/internal/service/desktop_update_service_test.go`
- `backend/internal/handler/admin/desktop_update_handler_test.go`
- `backend/internal/server/routes/desktop_updates_test.go`
- `frontend/src/views/admin/__tests__/DesktopUpdatesView.spec.ts`
- `desktop-client/src/platform/updater_test.rs`
- `desktop-client/src/lib.rs`

## Task 1: Build Desktop Update Metadata Storage And File Layout In The Backend

**Files:**
- Create: `backend/internal/service/desktop_update_models.go`
- Create: `backend/internal/service/desktop_update_service.go`
- Create: `backend/internal/service/desktop_update_service_test.go`
- Modify: `backend/internal/service/setting.go`
- Modify: `backend/internal/service/setting_service.go`

- [ ] **Step 1: Write the failing backend service tests**

```go
// backend/internal/service/desktop_update_service_test.go
package service

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

type desktopUpdateSettingRepoStub struct {
	values map[string]string
}

func (s *desktopUpdateSettingRepoStub) Get(ctx context.Context, key string) (*Setting, error) {
	value, ok := s.values[key]
	if !ok {
		return nil, ErrSettingNotFound
	}
	return &Setting{Key: key, Value: value}, nil
}

func (s *desktopUpdateSettingRepoStub) GetValue(ctx context.Context, key string) (string, error) {
	value, ok := s.values[key]
	if !ok {
		return "", ErrSettingNotFound
	}
	return value, nil
}

func (s *desktopUpdateSettingRepoStub) Set(ctx context.Context, key, value string) error {
	s.values[key] = value
	return nil
}

func (s *desktopUpdateSettingRepoStub) GetMultiple(ctx context.Context, keys []string) (map[string]string, error) {
	out := make(map[string]string, len(keys))
	for _, key := range keys {
		if value, ok := s.values[key]; ok {
			out[key] = value
		}
	}
	return out, nil
}

func (s *desktopUpdateSettingRepoStub) SetMultiple(ctx context.Context, settings map[string]string) error {
	for key, value := range settings {
		s.values[key] = value
	}
	return nil
}

func (s *desktopUpdateSettingRepoStub) GetAll(ctx context.Context) (map[string]string, error) {
	out := make(map[string]string, len(s.values))
	for key, value := range s.values {
		out[key] = value
	}
	return out, nil
}

func (s *desktopUpdateSettingRepoStub) Delete(ctx context.Context, key string) error {
	delete(s.values, key)
	return nil
}

func TestDesktopUpdateService_PublishAndCheckWindowsRelease(t *testing.T) {
	root := t.TempDir()
	repo := &desktopUpdateSettingRepoStub{values: map[string]string{}}
	svc := NewDesktopUpdateService(repo, root)

	packagePath := filepath.Join(root, "incoming", "installer.exe")
	require.NoError(t, os.MkdirAll(filepath.Dir(packagePath), 0o755))
	require.NoError(t, os.WriteFile(packagePath, []byte("payload"), 0o644))

	release, err := svc.CreateRelease(context.Background(), CreateDesktopReleaseInput{
		Version:                 "0.2.0",
		Platform:                "windows",
		Arch:                    "x64",
		Title:                   "发现新版本",
		Summary:                 "修复若干问题",
		ReleaseNotesMarkdown:    "## 新增\n- 更稳定",
		AnnouncementItems:       []DesktopAnnouncementItem{{Title: "维护提醒", Content: "本周五维护", Kind: "maintenance"}},
		Published:               true,
		ForceUpdate:             true,
		MinimumSupportedVersion: "0.1.0",
		PackageUploadPath:       packagePath,
	})
	require.NoError(t, err)
	require.NotZero(t, release.ID)
	require.Equal(t, "0.2.0", release.Version)
	require.NotEmpty(t, release.SHA256)
	require.FileExists(t, filepath.Join(root, "releases", release.ReleaseSlug, "installer.exe"))

	check, err := svc.CheckForClient(context.Background(), DesktopUpdateCheckInput{
		Platform:       "windows",
		Arch:           "x64",
		CurrentVersion: "0.1.0",
	})
	require.NoError(t, err)
	require.True(t, check.HasUpdate)
	require.True(t, check.ForceUpdate)
	require.Equal(t, "0.2.0", check.LatestVersion)
	require.Len(t, check.AnnouncementItems, 1)
}
```

- [ ] **Step 2: Run the focused Go test and verify it fails**

Run:

```powershell
cd backend
go test ./internal/service -run TestDesktopUpdateService_PublishAndCheckWindowsRelease -v
```

Expected: FAIL with errors such as `undefined: NewDesktopUpdateService`, `undefined: CreateDesktopReleaseInput`, and missing desktop update model types.

- [ ] **Step 3: Add setting keys and implement the metadata/file storage service**

```go
// backend/internal/service/setting.go
const (
	SettingKeyDesktopUpdateFeed = "desktop_update_feed"
)
```

```go
// backend/internal/service/desktop_update_models.go
package service

import "time"

type DesktopAnnouncementItem struct {
	Title     string     `json:"title"`
	Content   string     `json:"content"`
	Kind      string     `json:"kind"`
	Pinned    bool       `json:"pinned"`
	StartsAt  *time.Time `json:"starts_at,omitempty"`
	EndsAt    *time.Time `json:"ends_at,omitempty"`
}

type DesktopReleaseRecord struct {
	ID                      int64                   `json:"id"`
	ReleaseSlug             string                  `json:"release_slug"`
	Version                 string                  `json:"version"`
	Platform                string                  `json:"platform"`
	Arch                    string                  `json:"arch"`
	Title                   string                  `json:"title"`
	Summary                 string                  `json:"summary"`
	ReleaseNotesMarkdown    string                  `json:"release_notes_markdown"`
	AnnouncementItems       []DesktopAnnouncementItem `json:"announcement_items"`
	FileName                string                  `json:"file_name"`
	FileSize                int64                   `json:"file_size"`
	SHA256                  string                  `json:"sha256"`
	Published               bool                    `json:"published"`
	ForceUpdate             bool                    `json:"force_update"`
	MinimumSupportedVersion string                  `json:"minimum_supported_version"`
	PublishedAt             *time.Time              `json:"published_at,omitempty"`
	CreatedAt               time.Time               `json:"created_at"`
	UpdatedAt               time.Time               `json:"updated_at"`
}

type DesktopUpdateFeed struct {
	NextID   int64                  `json:"next_id"`
	Releases []DesktopReleaseRecord `json:"releases"`
}
```

```go
// backend/internal/service/desktop_update_service.go
package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"
)

type DesktopUpdateService struct {
	settingRepo SettingRepository
	rootDir     string
}

func NewDesktopUpdateService(settingRepo SettingRepository, rootDir string) *DesktopUpdateService {
	return &DesktopUpdateService{settingRepo: settingRepo, rootDir: rootDir}
}

func (s *DesktopUpdateService) releasesRoot() string {
	return filepath.Join(s.rootDir, "releases")
}

func (s *DesktopUpdateService) loadFeed(ctx context.Context) (*DesktopUpdateFeed, error) {
	raw, err := s.settingRepo.GetValue(ctx, SettingKeyDesktopUpdateFeed)
	if err != nil {
		if err == ErrSettingNotFound {
			return &DesktopUpdateFeed{NextID: 1, Releases: []DesktopReleaseRecord{}}, nil
		}
		return nil, fmt.Errorf("load desktop update feed: %w", err)
	}
	feed := &DesktopUpdateFeed{}
	if err := json.Unmarshal([]byte(raw), feed); err != nil {
		return nil, fmt.Errorf("decode desktop update feed: %w", err)
	}
	if feed.NextID == 0 {
		feed.NextID = int64(len(feed.Releases) + 1)
	}
	return feed, nil
}

func (s *DesktopUpdateService) saveFeed(ctx context.Context, feed *DesktopUpdateFeed) error {
	payload, err := json.Marshal(feed)
	if err != nil {
		return fmt.Errorf("encode desktop update feed: %w", err)
	}
	return s.settingRepo.Set(ctx, SettingKeyDesktopUpdateFeed, string(payload))
}

func fileSHA256(path string) (string, int64, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", 0, err
	}
	defer f.Close()

	h := sha256.New()
	n, err := io.Copy(h, f)
	if err != nil {
		return "", 0, err
	}
	return hex.EncodeToString(h.Sum(nil)), n, nil
}
```

- [ ] **Step 4: Re-run the service test and verify it passes**

Run:

```powershell
cd backend
go test ./internal/service -run TestDesktopUpdateService_PublishAndCheckWindowsRelease -v
```

Expected: PASS with the service persisting desktop update metadata and copying the uploaded Windows package into `backend/data/desktop-updates/releases/...`.

- [ ] **Step 5: Commit the storage/service foundation**

```bash
git add backend/internal/service/setting.go backend/internal/service/setting_service.go backend/internal/service/desktop_update_models.go backend/internal/service/desktop_update_service.go backend/internal/service/desktop_update_service_test.go
git commit -m "feat: add desktop update metadata service"
```

## Task 2: Expose Public Desktop Update Check And Package Download Endpoints

**Files:**
- Create: `backend/internal/handler/desktop_update_handler.go`
- Create: `backend/internal/handler/dto/desktop_update.go`
- Modify: `backend/internal/handler/handler.go`
- Modify: `backend/internal/handler/wire.go`
- Modify: `backend/internal/server/routes/desktop.go`
- Test: `backend/internal/server/routes/desktop_updates_test.go`

- [ ] **Step 1: Write the failing route test for the public desktop update API**

```go
// backend/internal/server/routes/desktop_updates_test.go
package routes

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/handler"
	"github.com/Wei-Shaw/sub2api/internal/service"
	servermiddleware "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

func TestDesktopUpdateRoutes_RegisterPublicCheckAndPackageEndpoints(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	v1 := r.Group("/api/v1")

	h := &handler.Handlers{
		DesktopUpdates: handler.NewDesktopUpdateHandler(&desktopUpdateRouteServiceStub{}),
	}

	RegisterDesktopRoutes(
		r,
		v1,
		h,
		servermiddleware.JWTAuthMiddleware(func(c *gin.Context) { c.Next() }),
		servermiddleware.DesktopRuntimeAuthMiddleware(func(c *gin.Context) { c.Next() }),
		nil,
	)

	req := httptest.NewRequest(http.MethodGet, "/api/v1/desktop/updates/check?platform=windows&arch=x64&current_version=0.1.0", nil)
	rec := httptest.NewRecorder()
	r.ServeHTTP(rec, req)

	require.NotEqual(t, http.StatusNotFound, rec.Code)
}

type desktopUpdateRouteServiceStub struct{}

func (s *desktopUpdateRouteServiceStub) CheckForClient(_ context.Context, _ service.DesktopUpdateCheckInput) (*service.DesktopUpdateCheckResult, error) {
	return &service.DesktopUpdateCheckResult{
		HasUpdate:      true,
		CurrentVersion: "0.1.0",
		LatestVersion:  "0.2.0",
		ReleaseID:      1,
		Title:          "发现新版本",
		Summary:        "修复若干问题",
		DownloadURL:    "/api/v1/desktop/updates/releases/1/package",
	}, nil
}
```

- [ ] **Step 2: Run the route test and verify it fails**

Run:

```powershell
cd backend
go test ./internal/server/routes -run TestDesktopUpdateRoutes_RegisterPublicCheckAndPackageEndpoints -v
```

Expected: FAIL because `Handlers` has no `DesktopUpdates` field and the `/desktop/updates/check` route does not exist.

- [ ] **Step 3: Add the public handler, DTOs, and route registrations**

```go
// backend/internal/handler/dto/desktop_update.go
package dto

type DesktopUpdateCheckResponse struct {
	HasUpdate         bool                     `json:"has_update"`
	CurrentVersion    string                   `json:"current_version"`
	LatestVersion     string                   `json:"latest_version"`
	ReleaseID         int64                    `json:"release_id"`
	ForceUpdate       bool                     `json:"force_update"`
	Title             string                   `json:"title"`
	Summary           string                   `json:"summary"`
	FileSize          int64                    `json:"file_size"`
	SHA256            string                   `json:"sha256"`
	DownloadURL       string                   `json:"download_url"`
	ReleaseNotes      string                   `json:"release_notes"`
	AnnouncementItems []service.DesktopAnnouncementItem `json:"announcement_items"`
}
```

```go
// backend/internal/handler/desktop_update_handler.go
package handler

import (
	"strconv"

	"github.com/Wei-Shaw/sub2api/internal/handler/dto"
	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type desktopUpdateService interface {
	CheckForClient(ctx context.Context, input service.DesktopUpdateCheckInput) (*service.DesktopUpdateCheckResult, error)
	GetPublishedRelease(ctx context.Context, releaseID int64) (*service.DesktopReleaseRecord, error)
	ServePackage(ctx context.Context, releaseID int64) (string, string, error)
}

type DesktopUpdateHandler struct {
	service desktopUpdateService
}

func NewDesktopUpdateHandler(service desktopUpdateService) *DesktopUpdateHandler {
	return &DesktopUpdateHandler{service: service}
}

func (h *DesktopUpdateHandler) Check(c *gin.Context) {
	result, err := h.service.CheckForClient(c.Request.Context(), service.DesktopUpdateCheckInput{
		Platform:       c.Query("platform"),
		Arch:           c.Query("arch"),
		CurrentVersion: c.Query("current_version"),
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, dto.DesktopUpdateCheckResponse{
		HasUpdate:         result.HasUpdate,
		CurrentVersion:    result.CurrentVersion,
		LatestVersion:     result.LatestVersion,
		ReleaseID:         result.ReleaseID,
		ForceUpdate:       result.ForceUpdate,
		Title:             result.Title,
		Summary:           result.Summary,
		FileSize:          result.FileSize,
		SHA256:            result.SHA256,
		DownloadURL:       result.DownloadURL,
		ReleaseNotes:      result.ReleaseNotes,
		AnnouncementItems: result.AnnouncementItems,
	})
}

func (h *DesktopUpdateHandler) DownloadPackage(c *gin.Context) {
	releaseID, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || releaseID <= 0 {
		response.BadRequest(c, "invalid release id")
		return
	}
	contentType, path, err := h.service.ServePackage(c.Request.Context(), releaseID)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	c.FileAttachment(path, filepath.Base(path))
	c.Header("Content-Type", contentType)
}
```

```go
// backend/internal/server/routes/desktop.go
publicUpdates := v1.Group("/desktop/updates")
{
	publicUpdates.GET("/check", h.DesktopUpdates.Check)
	publicUpdates.GET("/releases/:id", h.DesktopUpdates.GetRelease)
	publicUpdates.GET("/releases/:id/package", h.DesktopUpdates.DownloadPackage)
	publicUpdates.GET("/announcements", h.DesktopUpdates.ListAnnouncements)
}
```

- [ ] **Step 4: Run the route tests and a package compile check**

Run:

```powershell
cd backend
go test ./internal/server/routes -run TestDesktopUpdateRoutes_RegisterPublicCheckAndPackageEndpoints -v
go test ./internal/handler -run TestDesktopHandler -v
```

Expected: PASS for the new route test and no compile regressions in existing desktop handlers.

- [ ] **Step 5: Commit the public update API**

```bash
git add backend/internal/handler/desktop_update_handler.go backend/internal/handler/dto/desktop_update.go backend/internal/handler/handler.go backend/internal/handler/wire.go backend/internal/server/routes/desktop.go backend/internal/server/routes/desktop_updates_test.go
git commit -m "feat: add desktop update public api"
```

## Task 3: Add Admin CRUD, Upload, Publish, And Announcement Management Endpoints

**Files:**
- Create: `backend/internal/handler/admin/desktop_update_handler.go`
- Create: `backend/internal/handler/admin/desktop_update_handler_test.go`
- Modify: `backend/internal/handler/handler.go`
- Modify: `backend/internal/handler/wire.go`
- Modify: `backend/internal/server/routes/admin.go`

- [ ] **Step 1: Write the failing admin handler test**

```go
// backend/internal/handler/admin/desktop_update_handler_test.go
package admin

import (
	"bytes"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

func TestDesktopUpdateAdminHandler_CreateReleaseFromMultipartUpload(t *testing.T) {
	gin.SetMode(gin.TestMode)
	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	require.NoError(t, writer.WriteField("version", "0.2.0"))
	require.NoError(t, writer.WriteField("platform", "windows"))
	require.NoError(t, writer.WriteField("arch", "x64"))
	require.NoError(t, writer.WriteField("title", "发现新版本"))
	require.NoError(t, writer.WriteField("summary", "修复若干问题"))
	part, err := writer.CreateFormFile("package", "installer.exe")
	require.NoError(t, err)
	_, err = part.Write([]byte("payload"))
	require.NoError(t, err)
	require.NoError(t, writer.Close())

	rec := httptest.NewRecorder()
	ctx, r := gin.CreateTestContext(rec)
	req := httptest.NewRequest(http.MethodPost, "/api/v1/admin/desktop-updates/releases", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	ctx.Request = req

	handler := NewDesktopUpdateHandler(newDesktopUpdateServiceStub())
	handler.CreateRelease(ctx)

	require.Equal(t, http.StatusOK, rec.Code)
	require.Contains(t, rec.Body.String(), `"version":"0.2.0"`)
}
```

- [ ] **Step 2: Run the admin handler test and verify it fails**

Run:

```powershell
cd backend
go test ./internal/handler/admin -run TestDesktopUpdateAdminHandler_CreateReleaseFromMultipartUpload -v
```

Expected: FAIL because the admin desktop update handler does not exist.

- [ ] **Step 3: Implement admin endpoints and route registration**

```go
// backend/internal/handler/admin/desktop_update_handler.go
package admin

import (
	"net/http"
	"os"
	"path/filepath"

	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type desktopUpdateAdminService interface {
	ListReleases(ctx context.Context, page, pageSize int) ([]service.DesktopReleaseRecord, int, error)
	CreateRelease(ctx context.Context, input service.CreateDesktopReleaseInput) (*service.DesktopReleaseRecord, error)
	UpdateRelease(ctx context.Context, releaseID int64, input service.UpdateDesktopReleaseInput) (*service.DesktopReleaseRecord, error)
	DeleteRelease(ctx context.Context, releaseID int64) error
}

func (h *DesktopUpdateHandler) CreateRelease(c *gin.Context) {
	fileHeader, err := c.FormFile("package")
	if err != nil {
		response.BadRequest(c, "package file is required")
		return
	}
	tempDir, err := os.MkdirTemp("", "desktop-update-upload-*")
	if err != nil {
		response.Error(c, http.StatusInternalServerError, err.Error())
		return
	}
	defer os.RemoveAll(tempDir)

	dst := filepath.Join(tempDir, filepath.Base(fileHeader.Filename))
	if err := c.SaveUploadedFile(fileHeader, dst); err != nil {
		response.Error(c, http.StatusInternalServerError, err.Error())
		return
	}

	release, err := h.service.CreateRelease(c.Request.Context(), service.CreateDesktopReleaseInput{
		Version:                 c.PostForm("version"),
		Platform:                c.PostForm("platform"),
		Arch:                    c.PostForm("arch"),
		Title:                   c.PostForm("title"),
		Summary:                 c.PostForm("summary"),
		ReleaseNotesMarkdown:    c.PostForm("release_notes_markdown"),
		PackageUploadPath:       dst,
		Published:               c.PostForm("published") == "true",
		ForceUpdate:             c.PostForm("force_update") == "true",
		MinimumSupportedVersion: c.PostForm("minimum_supported_version"),
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, release)
}
```

```go
// backend/internal/server/routes/admin.go
func registerDesktopUpdateRoutes(admin *gin.RouterGroup, h *handler.Handlers) {
	desktopUpdates := admin.Group("/desktop-updates")
	{
		desktopUpdates.GET("/releases", h.Admin.DesktopUpdates.ListReleases)
		desktopUpdates.POST("/releases", h.Admin.DesktopUpdates.CreateRelease)
		desktopUpdates.GET("/releases/:id", h.Admin.DesktopUpdates.GetRelease)
		desktopUpdates.PUT("/releases/:id", h.Admin.DesktopUpdates.UpdateRelease)
		desktopUpdates.DELETE("/releases/:id", h.Admin.DesktopUpdates.DeleteRelease)
		desktopUpdates.GET("/announcements", h.Admin.DesktopUpdates.ListStandaloneAnnouncements)
		desktopUpdates.POST("/announcements", h.Admin.DesktopUpdates.CreateStandaloneAnnouncement)
		desktopUpdates.PUT("/announcements/:id", h.Admin.DesktopUpdates.UpdateStandaloneAnnouncement)
		desktopUpdates.DELETE("/announcements/:id", h.Admin.DesktopUpdates.DeleteStandaloneAnnouncement)
	}
}
```

- [ ] **Step 4: Run the admin tests and a targeted route compile**

Run:

```powershell
cd backend
go test ./internal/handler/admin -run TestDesktopUpdateAdminHandler_CreateReleaseFromMultipartUpload -v
go test ./internal/server/routes -run TestDesktopUpdateRoutes_RegisterPublicCheckAndPackageEndpoints -v
```

Expected: PASS for the multipart upload admin test and the public route test still green.

- [ ] **Step 5: Commit the admin desktop update API**

```bash
git add backend/internal/handler/admin/desktop_update_handler.go backend/internal/handler/admin/desktop_update_handler_test.go backend/internal/handler/handler.go backend/internal/handler/wire.go backend/internal/server/routes/admin.go
git commit -m "feat: add admin desktop update endpoints"
```

## Task 4: Build The Admin Desktop Update Center In Vue

**Files:**
- Create: `frontend/src/api/admin/desktopUpdates.ts`
- Create: `frontend/src/views/admin/DesktopUpdatesView.vue`
- Create: `frontend/src/views/admin/__tests__/DesktopUpdatesView.spec.ts`
- Modify: `frontend/src/api/admin/index.ts`
- Modify: `frontend/src/router/index.ts`
- Modify: `frontend/src/components/layout/AppSidebar.vue`
- Modify: `frontend/src/types/index.ts`
- Modify: `frontend/src/i18n/locales/zh.ts`
- Modify: `frontend/src/i18n/locales/en.ts`

- [ ] **Step 1: Write the failing Vue test for the new admin page**

```ts
// frontend/src/views/admin/__tests__/DesktopUpdatesView.spec.ts
import { render, screen } from '@testing-library/vue'
import { createPinia } from 'pinia'
import DesktopUpdatesView from '../DesktopUpdatesView.vue'

test('renders desktop update center actions', async () => {
  render(DesktopUpdatesView, {
    global: {
      plugins: [createPinia()],
      stubs: ['AppLayout', 'TablePageLayout', 'BaseDialog', 'ConfirmDialog'],
    },
  })

  expect(screen.getByText('桌面更新中心')).toBeInTheDocument()
  expect(screen.getByText('创建版本')).toBeInTheDocument()
  expect(screen.getByText('公告瀑布')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run the Vue test to verify it fails**

Run:

```powershell
cd frontend
pnpm vitest run src/views/admin/__tests__/DesktopUpdatesView.spec.ts
```

Expected: FAIL because `DesktopUpdatesView.vue` and its API module do not exist.

- [ ] **Step 3: Implement the admin API module, route, sidebar item, and page**

```ts
// frontend/src/api/admin/desktopUpdates.ts
import { apiClient } from '../client'
import type { BasePaginationResponse } from '@/types'

export interface DesktopRelease {
  id: number
  version: string
  platform: string
  arch: string
  title: string
  summary: string
  release_notes_markdown: string
  file_size: number
  sha256: string
  published: boolean
  force_update: boolean
  minimum_supported_version: string
  published_at?: string
  created_at: string
  updated_at: string
}

export async function listReleases(page = 1, pageSize = 20) {
  const { data } = await apiClient.get<BasePaginationResponse<DesktopRelease>>('/admin/desktop-updates/releases', {
    params: { page, page_size: pageSize },
  })
  return data
}
```

```ts
// frontend/src/router/index.ts
{
  path: '/admin/desktop-updates',
  name: 'AdminDesktopUpdates',
  component: () => import('@/views/admin/DesktopUpdatesView.vue'),
  meta: {
    requiresAuth: true,
    requiresAdmin: true,
    title: 'Desktop Updates',
    titleKey: 'admin.desktopUpdates.title',
    descriptionKey: 'admin.desktopUpdates.description',
  }
}
```

```vue
<!-- frontend/src/views/admin/DesktopUpdatesView.vue -->
<template>
  <AppLayout>
    <TablePageLayout>
      <template #filters>
        <div class="flex items-center justify-between gap-3">
          <div>
            <h1 class="text-2xl font-semibold text-gray-900 dark:text-white">桌面更新中心</h1>
            <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">管理 Windows 安装包、更新说明和公告瀑布。</p>
          </div>
          <button class="btn btn-primary" @click="openCreateDialog">创建版本</button>
        </div>
      </template>

      <template #table>
        <DataTable :columns="columns" :data="releases" :loading="loading">
          <template #cell-title="{ row }">
            <div class="min-w-0">
              <div class="font-medium text-gray-900 dark:text-white">{{ row.title }}</div>
              <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">v{{ row.version }} · {{ row.platform }}/{{ row.arch }}</div>
            </div>
          </template>
        </DataTable>
      </template>
    </TablePageLayout>

    <BaseDialog :show="showEditDialog" title="版本发布" width="wide" @close="closeEdit">
      <form id="desktop-update-form" class="space-y-4" @submit.prevent="handleSave">
        <input v-model="form.version" class="input" />
        <textarea v-model="form.release_notes_markdown" class="input" rows="8"></textarea>
        <input ref="packageInput" type="file" accept=".exe" class="input" />
      </form>
      <template #footer>
        <div class="flex justify-between gap-3">
          <button class="btn btn-secondary" type="button" @click="closeEdit">取消</button>
          <button class="btn btn-primary" type="submit" form="desktop-update-form">保存</button>
        </div>
      </template>
    </BaseDialog>
  </AppLayout>
</template>
```

- [ ] **Step 4: Run the Vue tests and a build-oriented type check**

Run:

```powershell
cd frontend
pnpm vitest run src/views/admin/__tests__/DesktopUpdatesView.spec.ts
pnpm tsc --noEmit
```

Expected:

- The `DesktopUpdatesView` test passes.
- `tsc --noEmit` succeeds with the new route, API types, and i18n keys.

- [ ] **Step 5: Commit the admin desktop update UI**

```bash
git add frontend/src/api/admin/desktopUpdates.ts frontend/src/api/admin/index.ts frontend/src/views/admin/DesktopUpdatesView.vue frontend/src/views/admin/__tests__/DesktopUpdatesView.spec.ts frontend/src/router/index.ts frontend/src/components/layout/AppSidebar.vue frontend/src/types/index.ts frontend/src/i18n/locales/zh.ts frontend/src/i18n/locales/en.ts
git commit -m "feat: add desktop update admin center"
```

## Task 5: Add Desktop Client Update API, View Model, And Global Dialog Wiring

**Files:**
- Create: `desktop-client/src/api/update.rs`
- Create: `desktop-client/src/app/view_models/update_vm.rs`
- Create: `desktop-client/ui/screens/update_dialog.slint`
- Modify: `desktop-client/src/app/mod.rs`
- Modify: `desktop-client/src/app/view_models/mod.rs`
- Modify: `desktop-client/src/lib.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/ui/app-window.slint`
- Modify: `desktop-client/ui/screens/help_detail.slint`

- [ ] **Step 1: Write the failing Rust tests for update API and dialog copy**

```rust
// desktop-client/src/app/view_models/update_vm.rs
#[cfg(test)]
mod tests {
    use super::{UpdateDialogState, UpdateViewModel};

    #[test]
    fn update_vm_hides_secondary_action_for_force_update() {
        let vm = UpdateViewModel::available(
            "0.1.0".to_string(),
            "0.2.0".to_string(),
            true,
            "发现新版本".to_string(),
            "当前版本已停止支持".to_string(),
        );

        assert_eq!(vm.state, UpdateDialogState::AvailableRequired);
        assert_eq!(vm.primary_text, "立即更新");
        assert_eq!(vm.secondary_text, None);
    }
}
```

```rust
// desktop-client/src/lib.rs
#[test]
fn desktop_update_copy_guard_uses_new_dialog_labels() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dialog =
        std::fs::read_to_string(manifest_dir.join("ui/screens/update_dialog.slint")).unwrap();
    let app_window =
        std::fs::read_to_string(manifest_dir.join("ui/app-window.slint")).unwrap();

    assert!(dialog.contains("发现新版本"));
    assert!(dialog.contains("立即更新"));
    assert!(dialog.contains("稍后"));
    assert!(app_window.contains("update-dialog-visible"));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib update_vm_hides_secondary_action_for_force_update
```

Expected: FAIL because the update API/view model/dialog files are missing.

- [ ] **Step 3: Implement the desktop update API and global dialog shell**

```rust
// desktop-client/src/api/update.rs
use serde::Deserialize;

use crate::api::http::ApiClient;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DesktopUpdateCheckResponse {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_id: i64,
    pub force_update: bool,
    pub title: String,
    pub summary: String,
    pub sha256: String,
    pub file_size: i64,
    pub download_url: String,
    pub release_notes: String,
}

pub fn check_desktop_update_blocking(
    client: &ApiClient,
    current_version: &str,
) -> anyhow::Result<DesktopUpdateCheckResponse> {
    client.get_json(
        "/desktop/updates/check",
        &[("platform", "windows"), ("arch", "x64"), ("current_version", current_version)],
    )
}
```

```rust
// desktop-client/src/app/view_models/update_vm.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDialogState {
    Hidden,
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
    pub title: String,
    pub current_version: String,
    pub latest_version: String,
    pub summary: String,
    pub force_update: bool,
    pub primary_text: String,
    pub secondary_text: Option<String>,
}

impl UpdateViewModel {
    pub fn available(
        current_version: String,
        latest_version: String,
        force_update: bool,
        title: String,
        summary: String,
    ) -> Self {
        Self {
            state: if force_update {
                UpdateDialogState::AvailableRequired
            } else {
                UpdateDialogState::AvailableOptional
            },
            title,
            current_version,
            latest_version,
            summary,
            force_update,
            primary_text: "立即更新".to_string(),
            secondary_text: if force_update { None } else { Some("稍后".to_string()) },
        }
    }
}
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
    callback primary-requested();
    callback secondary-requested();

    if root.visible: Rectangle {
        x: 56px;
        y: 56px;
        width: parent.width - 112px;
        height: parent.height - 112px;
        background: #ffffff;
        border-radius: 28px;

        Text { x: 28px; y: 24px; text: "发现新版本"; color: #17324a; font-size: 30px; font-weight: 800; }
        Text { x: 28px; y: 82px; text: root.latest-version; color: #2b59d0; font-size: 20px; font-weight: 700; }
        Text { x: 28px; y: 120px; text: "当前版本 " + root.current-version + "，新版本已可用。"; color: #617b92; font-size: 14px; }
        Text { x: 28px; y: 168px; width: parent.width - 56px; text: root.summary; wrap: word-wrap; }

        if !root.force-update: Button {
            x: parent.width - 236px;
            y: parent.height - 74px;
            width: 88px;
            height: 42px;
            text: "稍后";
            clicked => { root.secondary-requested(); }
        }

        Button {
            x: parent.width - 132px;
            y: parent.height - 74px;
            width: 104px;
            height: 42px;
            text: "立即更新";
            clicked => { root.primary-requested(); }
        }
    }
}
```

- [ ] **Step 4: Run the Rust tests and a compile check**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib update_vm_hides_secondary_action_for_force_update
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml --lib desktop_update_copy_guard_uses_new_dialog_labels
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
```

Expected: PASS for the update view model tests and `cargo check` green with the new dialog wiring.

- [ ] **Step 5: Commit the update UI shell**

```bash
git add desktop-client/src/api/update.rs desktop-client/src/app/mod.rs desktop-client/src/app/view_models/mod.rs desktop-client/src/app/view_models/update_vm.rs desktop-client/src/lib.rs desktop-client/src/main.rs desktop-client/ui/app-window.slint desktop-client/ui/screens/help_detail.slint desktop-client/ui/screens/update_dialog.slint
git commit -m "feat: add desktop update dialog shell"
```

## Task 6: Implement Windows Download, SHA256 Verification, Silent Install, And Relaunch

**Files:**
- Create: `desktop-client/src/platform/updater.rs`
- Create: `desktop-client/src/platform/updater_test.rs`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/src/storage/app_state.rs`
- Modify: `desktop-client/packaging/windows/desktop-client.iss`
- Modify: `desktop-client/README.md`

- [ ] **Step 1: Write the failing Rust tests for download verification and installer command generation**

```rust
// desktop-client/src/platform/updater_test.rs
use std::path::PathBuf;

use sub2api_desktop::platform::updater::{sha256_file, InstallerPlan};

#[test]
fn sha256_file_matches_known_payload() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("installer.exe");
    std::fs::write(&file, b"payload").unwrap();

    let hash = sha256_file(&file).unwrap();
    assert_eq!(
        hash,
        "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5"
    );
}

#[test]
fn installer_plan_uses_silent_inno_setup_arguments() {
    let plan = InstallerPlan::new(
        PathBuf::from("C:/tmp/update.exe"),
        PathBuf::from("C:/Users/test/AppData/Local/Programs/Sub2API Desktop Client/sub2api-desktop.exe"),
    );

    let args = plan.installer_args();
    assert!(args.contains(&"/VERYSILENT".to_string()));
    assert!(args.contains(&"/SUPPRESSMSGBOXES".to_string()));
    assert!(args.contains(&"/CLOSEAPPLICATIONS".to_string()));
    assert!(args.contains(&"/FORCECLOSEAPPLICATIONS".to_string()));
}
```

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml sha256_file_matches_known_payload -- --exact
```

Expected: FAIL because `platform::updater` does not exist yet.

- [ ] **Step 3: Implement the updater module and Windows install handoff**

```rust
// desktop-client/src/platform/updater.rs
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn download_installer(client: &Client, url: &str, dest: &Path) -> Result<()> {
    let mut response = client.get(url).send()?.error_for_status()?;
    let mut file = fs::File::create(dest)?;
    std::io::copy(&mut response, &mut file)?;
    file.flush()?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InstallerPlan {
    pub installer_path: PathBuf,
    pub installed_exe: PathBuf,
}

impl InstallerPlan {
    pub fn new(installer_path: PathBuf, installed_exe: PathBuf) -> Self {
        Self { installer_path, installed_exe }
    }

    pub fn installer_args(&self) -> Vec<String> {
        vec![
            "/VERYSILENT".to_string(),
            "/SUPPRESSMSGBOXES".to_string(),
            "/NORESTART".to_string(),
            "/SP-".to_string(),
            "/CLOSEAPPLICATIONS".to_string(),
            "/FORCECLOSEAPPLICATIONS".to_string(),
        ]
    }

    pub fn spawn_update_and_relaunch(&self, script_path: &Path) -> Result<()> {
        let script = format!(
            "$installer = '{}'\n$exe = '{}'\nStart-Process -FilePath $installer -ArgumentList @({}) -Wait -WindowStyle Hidden\nStart-Sleep -Seconds 2\nStart-Process -FilePath $exe -WindowStyle Hidden\n",
            self.installer_path.display(),
            self.installed_exe.display(),
            self.installer_args()
                .iter()
                .map(|arg| format!(\"'{}'\", arg))
                .collect::<Vec<_>>()
                .join(", ")
        );
        fs::write(script_path, script)?;
        Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script_path)
            .spawn()
            .context("spawn windows update helper")?;
        Ok(())
    }
}
```

```iss
; desktop-client/packaging/windows/desktop-client.iss
[Setup]
CloseApplications=yes
RestartApplications=no
UsePreviousAppDir=yes
```

```rust
// desktop-client/src/main.rs
let current_version = env!("CARGO_PKG_VERSION").to_string();
let update_client = ApiClient::new(config.api_base_url.clone());
if let Ok(check) = check_desktop_update_blocking(&update_client, &current_version) {
    if check.has_update {
        app.set_update_dialog_visible(true);
        app.set_update_force(check.force_update);
        app.set_update_current_version(SharedString::from(check.current_version));
        app.set_update_latest_version(SharedString::from(check.latest_version));
        app.set_update_summary(SharedString::from(check.summary));
    }
}
```

- [ ] **Step 4: Run tests, compile, and smoke-check the local desktop updater**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml sha256_file_matches_known_payload -- --exact
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml installer_plan_uses_silent_inno_setup_arguments -- --exact
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\start-desktop-client.ps1
```

Expected:

- The updater tests pass.
- `cargo check` succeeds.
- The desktop app boots, shows the update dialog when fed update data, and no compile/runtime regressions appear around the Windows helper script path.

- [ ] **Step 5: Commit the Windows updater**

```bash
git add desktop-client/src/platform/updater.rs desktop-client/src/platform/updater_test.rs desktop-client/src/main.rs desktop-client/src/storage/app_state.rs desktop-client/packaging/windows/desktop-client.iss desktop-client/README.md
git commit -m "feat: add windows desktop updater runtime"
```

## Task 7: Wire Admin Publishing To Client Checks And Run End-To-End Verification

**Files:**
- Modify: `backend/internal/handler/admin/desktop_update_handler_test.go`
- Modify: `frontend/src/views/admin/DesktopUpdatesView.vue`
- Modify: `desktop-client/src/main.rs`
- Modify: `desktop-client/src/platform/updater_test.rs`
- Modify: `docs/superpowers/specs/2026-04-18-windows-desktop-update-design.md` (only if validation notes need clarification)

- [ ] **Step 1: Add an end-to-end-ish backend test covering publish -> client check**

```go
// backend/internal/handler/admin/desktop_update_handler_test.go
func TestDesktopUpdatePublish_IsVisibleToClientCheck(t *testing.T) {
	root := t.TempDir()
	repo := &desktopUpdateSettingRepoStub{values: map[string]string{}}
	service := service.NewDesktopUpdateService(repo, root)

	packagePath := filepath.Join(root, "pkg.exe")
	require.NoError(t, os.WriteFile(packagePath, []byte("payload"), 0o644))

	_, err := service.CreateRelease(context.Background(), service.CreateDesktopReleaseInput{
		Version:              "0.3.0",
		Platform:             "windows",
		Arch:                 "x64",
		Title:                "发现新版本",
		Summary:              "新增更新中心",
		ReleaseNotesMarkdown: "## 新增\n- 更新中心",
		PackageUploadPath:    packagePath,
		Published:            true,
	})
	require.NoError(t, err)

	check, err := service.CheckForClient(context.Background(), service.DesktopUpdateCheckInput{
		Platform:       "windows",
		Arch:           "x64",
		CurrentVersion: "0.2.0",
	})
	require.NoError(t, err)
	require.True(t, check.HasUpdate)
	require.Equal(t, "0.3.0", check.LatestVersion)
}
```

- [ ] **Step 2: Run the backend and frontend verification suites**

Run:

```powershell
cd backend
go test ./internal/service ./internal/handler/admin ./internal/server/routes
cd ..\frontend
pnpm vitest run src/views/admin/__tests__/DesktopUpdatesView.spec.ts
pnpm tsc --noEmit
```

Expected:

- All targeted Go tests pass.
- The admin desktop update view tests pass.
- The frontend type check succeeds.

- [ ] **Step 3: Run desktop-client verification and an installer pipeline smoke test**

Run:

```powershell
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' test --manifest-path desktop-client/Cargo.toml
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' --config 'source.rsproxy-sparse.registry="sparse+https://rsproxy.cn/index/"' check --manifest-path desktop-client/Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\build-desktop-installer.ps1 -ApiBaseUrl "https://example.com/api/v1"
```

Expected:

- All desktop-client tests pass.
- `cargo check` succeeds.
- The installer build still completes and emits `Sub2API-Desktop-Setup-<version>.exe` plus `.sha256`, proving the updater changes did not break the packaging chain.

- [ ] **Step 4: Perform the manual validation checklist**

Run / verify:

```text
1. 后台创建并发布一个 Windows 版本，确认安装包、sha256、最低支持版本、公告瀑布保存成功。
2. 旧版本客户端启动后收到可选更新弹窗，点击“稍后”仍可进入主界面。
3. 将发布记录改成强制更新后，旧版本客户端启动被拦在更新弹窗，无法进入主界面。
4. 点击“立即更新”后看到下载进度，下载完成执行 sha256 校验。
5. 校验通过后触发静默安装，安装完成自动重启到新版客户端。
6. 将 sha256 改错后再次测试，客户端必须停在校验失败状态且不能安装。
```

Expected: All six manual checks pass with no bypass for force-update and no install allowed after checksum mismatch.

- [ ] **Step 5: Commit the verified update system**

```bash
git add backend frontend desktop-client
git commit -m "feat: add windows desktop update system"
```
