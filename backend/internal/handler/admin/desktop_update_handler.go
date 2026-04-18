package admin

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/Wei-Shaw/sub2api/internal/pkg/response"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

type desktopUpdateAdminService interface {
	ListReleases(ctx context.Context, page, pageSize int) ([]service.DesktopReleaseRecord, int64, error)
	CreateRelease(ctx context.Context, input service.CreateDesktopReleaseInput) (*service.DesktopReleaseRecord, error)
	GetReleaseByID(ctx context.Context, releaseID int64) (*service.DesktopReleaseRecord, error)
	UpdateRelease(ctx context.Context, releaseID int64, input service.UpdateDesktopReleaseInput) (*service.DesktopReleaseRecord, error)
	DeleteRelease(ctx context.Context, releaseID int64) error
}

type DesktopUpdateHandler struct {
	service desktopUpdateAdminService
}

func NewDesktopUpdateHandler(service desktopUpdateAdminService) *DesktopUpdateHandler {
	return &DesktopUpdateHandler{service: service}
}

func (h *DesktopUpdateHandler) ListReleases(c *gin.Context) {
	page, pageSize := response.ParsePagination(c)
	items, total, err := h.service.ListReleases(c.Request.Context(), page, pageSize)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Paginated(c, items, total, page, pageSize)
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
	defer func() { _ = os.RemoveAll(tempDir) }()

	dst := filepath.Join(tempDir, filepath.Base(fileHeader.Filename))
	if err := c.SaveUploadedFile(fileHeader, dst); err != nil {
		response.Error(c, http.StatusInternalServerError, err.Error())
		return
	}

	release, err := h.service.CreateRelease(c.Request.Context(), service.CreateDesktopReleaseInput{
		Version:                 strings.TrimSpace(c.PostForm("version")),
		Platform:                strings.TrimSpace(c.PostForm("platform")),
		Arch:                    strings.TrimSpace(c.PostForm("arch")),
		Title:                   strings.TrimSpace(c.PostForm("title")),
		Summary:                 strings.TrimSpace(c.PostForm("summary")),
		ReleaseNotesMarkdown:    c.PostForm("release_notes_markdown"),
		AnnouncementItems:       parseAnnouncementItems(c.PostForm("announcement_items")),
		PackageUploadPath:       dst,
		Published:               parseBoolForm(c.PostForm("published")),
		ForceUpdate:             parseBoolForm(c.PostForm("force_update")),
		MinimumSupportedVersion: strings.TrimSpace(c.PostForm("minimum_supported_version")),
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, release)
}

func (h *DesktopUpdateHandler) GetRelease(c *gin.Context) {
	releaseID, ok := parseAdminDesktopReleaseID(c)
	if !ok {
		return
	}

	release, err := h.service.GetReleaseByID(c.Request.Context(), releaseID)
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, release)
}

func (h *DesktopUpdateHandler) UpdateRelease(c *gin.Context) {
	releaseID, ok := parseAdminDesktopReleaseID(c)
	if !ok {
		return
	}

	var req struct {
		Version                 *string                            `json:"version"`
		Title                   *string                            `json:"title"`
		Summary                 *string                            `json:"summary"`
		ReleaseNotesMarkdown    *string                            `json:"release_notes_markdown"`
		AnnouncementItems       *[]service.DesktopAnnouncementItem `json:"announcement_items"`
		Published               *bool                              `json:"published"`
		ForceUpdate             *bool                              `json:"force_update"`
		MinimumSupportedVersion *string                            `json:"minimum_supported_version"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		response.BadRequest(c, "Invalid request: "+err.Error())
		return
	}

	release, err := h.service.UpdateRelease(c.Request.Context(), releaseID, service.UpdateDesktopReleaseInput{
		Version:                 req.Version,
		Title:                   req.Title,
		Summary:                 req.Summary,
		ReleaseNotesMarkdown:    req.ReleaseNotesMarkdown,
		AnnouncementItems:       req.AnnouncementItems,
		Published:               req.Published,
		ForceUpdate:             req.ForceUpdate,
		MinimumSupportedVersion: req.MinimumSupportedVersion,
	})
	if err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, release)
}

func (h *DesktopUpdateHandler) DeleteRelease(c *gin.Context) {
	releaseID, ok := parseAdminDesktopReleaseID(c)
	if !ok {
		return
	}

	if err := h.service.DeleteRelease(c.Request.Context(), releaseID); err != nil {
		response.ErrorFrom(c, err)
		return
	}
	response.Success(c, gin.H{"message": "desktop release deleted"})
}

func (h *DesktopUpdateHandler) ListStandaloneAnnouncements(c *gin.Context) {
	response.Success(c, []service.DesktopAnnouncementItem{})
}

func (h *DesktopUpdateHandler) CreateStandaloneAnnouncement(c *gin.Context) {
	response.Error(c, http.StatusNotImplemented, "standalone desktop announcements are not implemented yet")
}

func (h *DesktopUpdateHandler) UpdateStandaloneAnnouncement(c *gin.Context) {
	response.Error(c, http.StatusNotImplemented, "standalone desktop announcements are not implemented yet")
}

func (h *DesktopUpdateHandler) DeleteStandaloneAnnouncement(c *gin.Context) {
	response.Error(c, http.StatusNotImplemented, "standalone desktop announcements are not implemented yet")
}

func parseAdminDesktopReleaseID(c *gin.Context) (int64, bool) {
	releaseID, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || releaseID <= 0 {
		response.BadRequest(c, "invalid release id")
		return 0, false
	}
	return releaseID, true
}

func parseBoolForm(raw string) bool {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "1", "true", "yes", "on":
		return true
	default:
		return false
	}
}

func parseAnnouncementItems(raw string) []service.DesktopAnnouncementItem {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}

	var items []service.DesktopAnnouncementItem
	if err := json.Unmarshal([]byte(raw), &items); err != nil {
		return nil
	}
	return items
}
