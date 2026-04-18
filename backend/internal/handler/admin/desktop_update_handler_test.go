package admin

import (
	"bytes"
	"context"
	"encoding/json"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/service"
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
	ctx, _ := gin.CreateTestContext(rec)
	req := httptest.NewRequest(http.MethodPost, "/api/v1/admin/desktop-updates/releases", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	ctx.Request = req

	handler := NewDesktopUpdateHandler(newDesktopUpdateServiceStub())
	handler.CreateRelease(ctx)

	require.Equal(t, http.StatusOK, rec.Code)
	require.Contains(t, rec.Body.String(), `"version":"0.2.0"`)
}

func TestDesktopUpdateAdminHandler_CreateStandaloneAnnouncement(t *testing.T) {
	gin.SetMode(gin.TestMode)
	rec := httptest.NewRecorder()
	ctx, _ := gin.CreateTestContext(rec)
	req := httptest.NewRequest(
		http.MethodPost,
		"/api/v1/admin/desktop-updates/announcements",
		bytes.NewBufferString(`{"title":"维护提醒","content":"今晚维护","kind":"maintenance","pinned":true}`),
	)
	req.Header.Set("Content-Type", "application/json")
	ctx.Request = req

	handler := NewDesktopUpdateHandler(newDesktopUpdateServiceStub())
	handler.CreateStandaloneAnnouncement(ctx)

	require.Equal(t, http.StatusOK, rec.Code)

	var payload struct {
		Data service.DesktopStandaloneAnnouncementRecord `json:"data"`
	}
	require.NoError(t, json.Unmarshal(rec.Body.Bytes(), &payload))
	require.Equal(t, "维护提醒", payload.Data.Title)
	require.True(t, payload.Data.Pinned)
}

type desktopUpdateServiceStub struct{}

func newDesktopUpdateServiceStub() *desktopUpdateServiceStub {
	return &desktopUpdateServiceStub{}
}

func (s *desktopUpdateServiceStub) ListReleases(_ context.Context, _, _ int) ([]service.DesktopReleaseRecord, int64, error) {
	return []service.DesktopReleaseRecord{}, 0, nil
}

func (s *desktopUpdateServiceStub) CreateRelease(_ context.Context, input service.CreateDesktopReleaseInput) (*service.DesktopReleaseRecord, error) {
	return &service.DesktopReleaseRecord{
		ID:          1,
		ReleaseSlug: "windows-x64-0-2-0-1",
		Version:     input.Version,
		Platform:    input.Platform,
		Arch:        input.Arch,
		Title:       input.Title,
		Summary:     input.Summary,
		FileName:    "installer.exe",
		FileSize:    7,
		SHA256:      "abc",
		Published:   input.Published,
	}, nil
}

func (s *desktopUpdateServiceStub) GetReleaseByID(_ context.Context, releaseID int64) (*service.DesktopReleaseRecord, error) {
	return &service.DesktopReleaseRecord{ID: releaseID, Version: "0.2.0"}, nil
}

func (s *desktopUpdateServiceStub) UpdateRelease(_ context.Context, releaseID int64, _ service.UpdateDesktopReleaseInput) (*service.DesktopReleaseRecord, error) {
	return &service.DesktopReleaseRecord{ID: releaseID, Version: "0.2.0"}, nil
}

func (s *desktopUpdateServiceStub) DeleteRelease(_ context.Context, _ int64) error {
	return nil
}

func (s *desktopUpdateServiceStub) ListStandaloneAnnouncements(_ context.Context) ([]service.DesktopStandaloneAnnouncementRecord, error) {
	return []service.DesktopStandaloneAnnouncementRecord{{
		ID:      1,
		Title:   "维护提醒",
		Content: "今晚维护",
		Kind:    "maintenance",
		Pinned:  true,
	}}, nil
}

func (s *desktopUpdateServiceStub) CreateStandaloneAnnouncement(_ context.Context, input service.CreateDesktopStandaloneAnnouncementInput) (*service.DesktopStandaloneAnnouncementRecord, error) {
	return &service.DesktopStandaloneAnnouncementRecord{
		ID:      1,
		Title:   input.Title,
		Content: input.Content,
		Kind:    input.Kind,
		Pinned:  input.Pinned,
	}, nil
}

func (s *desktopUpdateServiceStub) UpdateStandaloneAnnouncement(_ context.Context, announcementID int64, input service.UpdateDesktopStandaloneAnnouncementInput) (*service.DesktopStandaloneAnnouncementRecord, error) {
	item := &service.DesktopStandaloneAnnouncementRecord{ID: announcementID}
	if input.Title != nil {
		item.Title = *input.Title
	}
	if input.Content != nil {
		item.Content = *input.Content
	}
	if input.Kind != nil {
		item.Kind = *input.Kind
	}
	if input.Pinned != nil {
		item.Pinned = *input.Pinned
	}
	return item, nil
}

func (s *desktopUpdateServiceStub) DeleteStandaloneAnnouncement(_ context.Context, _ int64) error {
	return nil
}
