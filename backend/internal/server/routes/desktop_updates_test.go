package routes

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/handler"
	servermiddleware "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
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

func (s *desktopUpdateRouteServiceStub) GetPublishedRelease(_ context.Context, releaseID int64) (*service.DesktopReleaseRecord, error) {
	return &service.DesktopReleaseRecord{
		ID:          releaseID,
		Version:     "0.2.0",
		Platform:    "windows",
		Arch:        "x64",
		Title:       "发现新版本",
		Summary:     "修复若干问题",
		Published:   true,
		FileName:    "installer.exe",
		FileSize:    7,
		SHA256:      "abc",
		ReleaseSlug: "windows-x64-0-2-0-1",
	}, nil
}

func (s *desktopUpdateRouteServiceStub) ListAnnouncements(_ context.Context, _, _ string) ([]service.DesktopAnnouncementItem, error) {
	return []service.DesktopAnnouncementItem{{
		Title:   "维护提醒",
		Content: "本周五维护",
		Kind:    "maintenance",
	}}, nil
}

func (s *desktopUpdateRouteServiceStub) ServePackage(_ context.Context, _ int64) (string, string, error) {
	return "application/octet-stream", "C:/tmp/installer.exe", nil
}
