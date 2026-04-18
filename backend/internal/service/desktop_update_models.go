package service

import "time"

type DesktopAnnouncementItem struct {
	Title    string     `json:"title"`
	Content  string     `json:"content"`
	Kind     string     `json:"kind"`
	Pinned   bool       `json:"pinned"`
	StartsAt *time.Time `json:"starts_at,omitempty"`
	EndsAt   *time.Time `json:"ends_at,omitempty"`
}

type DesktopReleaseRecord struct {
	ID                      int64                     `json:"id"`
	ReleaseSlug             string                    `json:"release_slug"`
	Version                 string                    `json:"version"`
	Platform                string                    `json:"platform"`
	Arch                    string                    `json:"arch"`
	Title                   string                    `json:"title"`
	Summary                 string                    `json:"summary"`
	ReleaseNotesMarkdown    string                    `json:"release_notes_markdown"`
	AnnouncementItems       []DesktopAnnouncementItem `json:"announcement_items"`
	FileName                string                    `json:"file_name"`
	FileSize                int64                     `json:"file_size"`
	SHA256                  string                    `json:"sha256"`
	Published               bool                      `json:"published"`
	ForceUpdate             bool                      `json:"force_update"`
	MinimumSupportedVersion string                    `json:"minimum_supported_version"`
	PublishedAt             *time.Time                `json:"published_at,omitempty"`
	CreatedAt               time.Time                 `json:"created_at"`
	UpdatedAt               time.Time                 `json:"updated_at"`
}

type DesktopUpdateFeed struct {
	NextID   int64                  `json:"next_id"`
	Releases []DesktopReleaseRecord `json:"releases"`
}

type CreateDesktopReleaseInput struct {
	Version                 string
	Platform                string
	Arch                    string
	Title                   string
	Summary                 string
	ReleaseNotesMarkdown    string
	AnnouncementItems       []DesktopAnnouncementItem
	Published               bool
	ForceUpdate             bool
	MinimumSupportedVersion string
	PackageUploadPath       string
}

type DesktopUpdateCheckInput struct {
	Platform       string
	Arch           string
	CurrentVersion string
}

type DesktopUpdateCheckResult struct {
	HasUpdate         bool
	CurrentVersion    string
	LatestVersion     string
	ReleaseID         int64
	ForceUpdate       bool
	Title             string
	Summary           string
	FileSize          int64
	SHA256            string
	DownloadURL       string
	ReleaseNotes      string
	AnnouncementItems []DesktopAnnouncementItem
}
