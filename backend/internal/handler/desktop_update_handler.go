package handler

import (
	"context"
	"path/filepath"
	"strconv"

	"github.com/Wei-Shaw/sub2api/internal/handler/dto"
	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type desktopUpdateService interface {
	CheckForClient(ctx context.Context, input service.DesktopUpdateCheckInput) (*service.DesktopUpdateCheckResult, error)
	GetPublishedRelease(ctx context.Context, releaseID int64) (*service.DesktopReleaseRecord, error)
	ListAnnouncements(ctx context.Context, platform, arch string) ([]service.DesktopAnnouncementItem, error)
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

func (h *DesktopUpdateHandler) GetRelease(c *gin.Context) {
	releaseID, ok := parseDesktopReleaseID(c)
	if !ok {
		return
	}

	record, err := h.service.GetPublishedRelease(c.Request.Context(), releaseID)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, record)
}

func (h *DesktopUpdateHandler) ListAnnouncements(c *gin.Context) {
	items, err := h.service.ListAnnouncements(c.Request.Context(), c.Query("platform"), c.Query("arch"))
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, items)
}

func (h *DesktopUpdateHandler) DownloadPackage(c *gin.Context) {
	releaseID, ok := parseDesktopReleaseID(c)
	if !ok {
		return
	}

	contentType, path, err := h.service.ServePackage(c.Request.Context(), releaseID)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}

	c.Header("Content-Type", contentType)
	c.FileAttachment(path, filepath.Base(path))
}

func parseDesktopReleaseID(c *gin.Context) (int64, bool) {
	releaseID, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || releaseID <= 0 {
		response.BadRequest(c, "invalid release id")
		return 0, false
	}
	return releaseID, true
}
