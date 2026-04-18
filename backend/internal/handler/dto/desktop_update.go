package dto

import "github.com/Wei-Shaw/sub2api/internal/service"

type DesktopUpdateCheckResponse struct {
	HasUpdate         bool                              `json:"has_update"`
	CurrentVersion    string                            `json:"current_version"`
	LatestVersion     string                            `json:"latest_version"`
	ReleaseID         int64                             `json:"release_id"`
	ForceUpdate       bool                              `json:"force_update"`
	Title             string                            `json:"title"`
	Summary           string                            `json:"summary"`
	FileSize          int64                             `json:"file_size"`
	SHA256            string                            `json:"sha256"`
	DownloadURL       string                            `json:"download_url"`
	ReleaseNotes      string                            `json:"release_notes"`
	AnnouncementItems []service.DesktopAnnouncementItem `json:"announcement_items"`
}
