package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime"
	"os"
	"path/filepath"
	"strings"
	"time"

	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
)

type DesktopUpdateService struct {
	settingRepo SettingRepository
	rootDir     string
}

var ErrDesktopReleaseNotFound = infraerrors.NotFound("DESKTOP_RELEASE_NOT_FOUND", "desktop release not found")

func NewDesktopUpdateService(settingRepo SettingRepository, rootDir string) *DesktopUpdateService {
	return &DesktopUpdateService{
		settingRepo: settingRepo,
		rootDir:     rootDir,
	}
}

func (s *DesktopUpdateService) releasesRoot() string {
	return filepath.Join(s.rootDir, "releases")
}

func (s *DesktopUpdateService) loadFeed(ctx context.Context) (*DesktopUpdateFeed, error) {
	raw, err := s.settingRepo.GetValue(ctx, SettingKeyDesktopUpdateFeed)
	if err != nil {
		if err == ErrSettingNotFound {
			return &DesktopUpdateFeed{
				NextID:                  1,
				NextAnnouncementID:      1,
				Releases:                []DesktopReleaseRecord{},
				StandaloneAnnouncements: []DesktopStandaloneAnnouncementRecord{},
			}, nil
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
	if feed.NextAnnouncementID == 0 {
		feed.NextAnnouncementID = int64(len(feed.StandaloneAnnouncements) + 1)
	}
	if feed.Releases == nil {
		feed.Releases = []DesktopReleaseRecord{}
	}
	if feed.StandaloneAnnouncements == nil {
		feed.StandaloneAnnouncements = []DesktopStandaloneAnnouncementRecord{}
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

func (s *DesktopUpdateService) CreateRelease(ctx context.Context, input CreateDesktopReleaseInput) (*DesktopReleaseRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	now := time.Now().UTC()
	releaseSlug := buildDesktopReleaseSlug(feed.NextID, input.Version, input.Platform, input.Arch)
	fileName := filepath.Base(input.PackageUploadPath)
	releaseDir := filepath.Join(s.releasesRoot(), releaseSlug)
	if err := os.MkdirAll(releaseDir, 0o755); err != nil {
		return nil, fmt.Errorf("create desktop release dir: %w", err)
	}

	targetPath := filepath.Join(releaseDir, fileName)
	if err := copyDesktopReleaseFile(input.PackageUploadPath, targetPath); err != nil {
		return nil, fmt.Errorf("store desktop release package: %w", err)
	}

	fileHash, fileSize, err := fileSHA256(targetPath)
	if err != nil {
		return nil, fmt.Errorf("hash desktop release package: %w", err)
	}

	var publishedAt *time.Time
	if input.Published {
		publishedAt = &now
	}

	record := DesktopReleaseRecord{
		ID:                      feed.NextID,
		ReleaseSlug:             releaseSlug,
		Version:                 strings.TrimSpace(input.Version),
		Platform:                strings.TrimSpace(input.Platform),
		Arch:                    strings.TrimSpace(input.Arch),
		Title:                   strings.TrimSpace(input.Title),
		Summary:                 strings.TrimSpace(input.Summary),
		ReleaseNotesMarkdown:    input.ReleaseNotesMarkdown,
		AnnouncementItems:       append([]DesktopAnnouncementItem(nil), input.AnnouncementItems...),
		FileName:                fileName,
		FileSize:                fileSize,
		SHA256:                  fileHash,
		Published:               input.Published,
		ForceUpdate:             input.ForceUpdate,
		MinimumSupportedVersion: strings.TrimSpace(input.MinimumSupportedVersion),
		PublishedAt:             publishedAt,
		CreatedAt:               now,
		UpdatedAt:               now,
	}

	feed.Releases = append(feed.Releases, record)
	feed.NextID++
	if err := s.saveFeed(ctx, feed); err != nil {
		return nil, err
	}
	return &record, nil
}

func (s *DesktopUpdateService) ListReleases(ctx context.Context, page, pageSize int) ([]DesktopReleaseRecord, int64, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, 0, err
	}

	records := append([]DesktopReleaseRecord(nil), feed.Releases...)
	sortDesktopReleases(records)
	total := int64(len(records))
	if page <= 0 {
		page = 1
	}
	if pageSize <= 0 {
		pageSize = 20
	}

	start := (page - 1) * pageSize
	if start >= len(records) {
		return []DesktopReleaseRecord{}, total, nil
	}
	end := start + pageSize
	if end > len(records) {
		end = len(records)
	}
	return records[start:end], total, nil
}

func (s *DesktopUpdateService) GetReleaseByID(ctx context.Context, releaseID int64) (*DesktopReleaseRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	index := desktopReleaseIndexByID(feed.Releases, releaseID)
	if index < 0 {
		return nil, ErrDesktopReleaseNotFound
	}
	release := feed.Releases[index]
	return &release, nil
}

func (s *DesktopUpdateService) CheckForClient(ctx context.Context, input DesktopUpdateCheckInput) (*DesktopUpdateCheckResult, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	currentVersion := strings.TrimSpace(input.CurrentVersion)
	latest := latestPublishedDesktopRelease(feed.Releases, input.Platform, input.Arch)
	if latest == nil {
		return &DesktopUpdateCheckResult{
			HasUpdate:      false,
			CurrentVersion: currentVersion,
			LatestVersion:  currentVersion,
		}, nil
	}

	hasUpdate := compareVersions(currentVersion, latest.Version) < 0
	return &DesktopUpdateCheckResult{
		HasUpdate:         hasUpdate,
		CurrentVersion:    currentVersion,
		LatestVersion:     latest.Version,
		ReleaseID:         latest.ID,
		ForceUpdate:       hasUpdate && latest.ForceUpdate,
		Title:             latest.Title,
		Summary:           latest.Summary,
		FileSize:          latest.FileSize,
		SHA256:            latest.SHA256,
		DownloadURL:       fmt.Sprintf("/api/v1/desktop/updates/releases/%d/package", latest.ID),
		ReleaseNotes:      latest.ReleaseNotesMarkdown,
		AnnouncementItems: append([]DesktopAnnouncementItem(nil), latest.AnnouncementItems...),
	}, nil
}

func (s *DesktopUpdateService) GetPublishedRelease(ctx context.Context, releaseID int64) (*DesktopReleaseRecord, error) {
	release, err := s.GetReleaseByID(ctx, releaseID)
	if err != nil {
		return nil, err
	}
	if !release.Published {
		return nil, ErrDesktopReleaseNotFound
	}
	return release, nil
}

func (s *DesktopUpdateService) UpdateRelease(ctx context.Context, releaseID int64, input UpdateDesktopReleaseInput) (*DesktopReleaseRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	index := desktopReleaseIndexByID(feed.Releases, releaseID)
	if index < 0 {
		return nil, ErrDesktopReleaseNotFound
	}

	record := &feed.Releases[index]
	if input.Version != nil {
		record.Version = strings.TrimSpace(*input.Version)
	}
	if input.Title != nil {
		record.Title = strings.TrimSpace(*input.Title)
	}
	if input.Summary != nil {
		record.Summary = strings.TrimSpace(*input.Summary)
	}
	if input.ReleaseNotesMarkdown != nil {
		record.ReleaseNotesMarkdown = *input.ReleaseNotesMarkdown
	}
	if input.AnnouncementItems != nil {
		record.AnnouncementItems = append([]DesktopAnnouncementItem(nil), (*input.AnnouncementItems)...)
	}
	if input.ForceUpdate != nil {
		record.ForceUpdate = *input.ForceUpdate
	}
	if input.MinimumSupportedVersion != nil {
		record.MinimumSupportedVersion = strings.TrimSpace(*input.MinimumSupportedVersion)
	}
	if input.Published != nil {
		record.Published = *input.Published
		if record.Published {
			now := time.Now().UTC()
			record.PublishedAt = &now
		} else {
			record.PublishedAt = nil
		}
	}
	record.UpdatedAt = time.Now().UTC()

	if err := s.saveFeed(ctx, feed); err != nil {
		return nil, err
	}
	release := *record
	return &release, nil
}

func (s *DesktopUpdateService) DeleteRelease(ctx context.Context, releaseID int64) error {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return err
	}

	index := desktopReleaseIndexByID(feed.Releases, releaseID)
	if index < 0 {
		return ErrDesktopReleaseNotFound
	}
	release := feed.Releases[index]
	feed.Releases = append(feed.Releases[:index], feed.Releases[index+1:]...)
	if err := s.saveFeed(ctx, feed); err != nil {
		return err
	}
	if release.ReleaseSlug != "" {
		if err := os.RemoveAll(filepath.Join(s.releasesRoot(), release.ReleaseSlug)); err != nil {
			return fmt.Errorf("remove desktop release dir: %w", err)
		}
	}
	return nil
}

func (s *DesktopUpdateService) ListStandaloneAnnouncements(ctx context.Context) ([]DesktopStandaloneAnnouncementRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	items := append([]DesktopStandaloneAnnouncementRecord(nil), feed.StandaloneAnnouncements...)
	sortDesktopStandaloneAnnouncements(items)
	return items, nil
}

func (s *DesktopUpdateService) CreateStandaloneAnnouncement(ctx context.Context, input CreateDesktopStandaloneAnnouncementInput) (*DesktopStandaloneAnnouncementRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	now := time.Now().UTC()
	record := DesktopStandaloneAnnouncementRecord{
		ID:        feed.NextAnnouncementID,
		ReleaseID: input.ReleaseID,
		Title:     strings.TrimSpace(input.Title),
		Content:   strings.TrimSpace(input.Content),
		Kind:      strings.TrimSpace(input.Kind),
		Pinned:    input.Pinned,
		StartsAt:  input.StartsAt,
		EndsAt:    input.EndsAt,
		CreatedAt: now,
		UpdatedAt: now,
	}

	feed.StandaloneAnnouncements = append(feed.StandaloneAnnouncements, record)
	feed.NextAnnouncementID++
	if err := s.saveFeed(ctx, feed); err != nil {
		return nil, err
	}
	return &record, nil
}

func (s *DesktopUpdateService) UpdateStandaloneAnnouncement(ctx context.Context, announcementID int64, input UpdateDesktopStandaloneAnnouncementInput) (*DesktopStandaloneAnnouncementRecord, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	index := desktopStandaloneAnnouncementIndexByID(feed.StandaloneAnnouncements, announcementID)
	if index < 0 {
		return nil, ErrDesktopReleaseNotFound
	}

	record := &feed.StandaloneAnnouncements[index]
	if input.ReleaseID != nil {
		record.ReleaseID = input.ReleaseID
	}
	if input.Title != nil {
		record.Title = strings.TrimSpace(*input.Title)
	}
	if input.Content != nil {
		record.Content = strings.TrimSpace(*input.Content)
	}
	if input.Kind != nil {
		record.Kind = strings.TrimSpace(*input.Kind)
	}
	if input.Pinned != nil {
		record.Pinned = *input.Pinned
	}
	if input.StartsAt != nil {
		record.StartsAt = *input.StartsAt
	}
	if input.EndsAt != nil {
		record.EndsAt = *input.EndsAt
	}
	record.UpdatedAt = time.Now().UTC()

	if err := s.saveFeed(ctx, feed); err != nil {
		return nil, err
	}
	updated := *record
	return &updated, nil
}

func (s *DesktopUpdateService) DeleteStandaloneAnnouncement(ctx context.Context, announcementID int64) error {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return err
	}

	index := desktopStandaloneAnnouncementIndexByID(feed.StandaloneAnnouncements, announcementID)
	if index < 0 {
		return ErrDesktopReleaseNotFound
	}
	feed.StandaloneAnnouncements = append(
		feed.StandaloneAnnouncements[:index],
		feed.StandaloneAnnouncements[index+1:]...,
	)
	return s.saveFeed(ctx, feed)
}

func (s *DesktopUpdateService) ListAnnouncements(ctx context.Context, platform, arch string) ([]DesktopAnnouncementItem, error) {
	feed, err := s.loadFeed(ctx)
	if err != nil {
		return nil, err
	}

	now := time.Now().UTC()
	items := make([]DesktopAnnouncementItem, 0, len(feed.StandaloneAnnouncements)+4)
	for i := range feed.StandaloneAnnouncements {
		record := feed.StandaloneAnnouncements[i]
		if !isStandaloneAnnouncementActive(record, now) {
			continue
		}
		items = append(items, DesktopAnnouncementItem{
			Title:    record.Title,
			Content:  record.Content,
			Kind:     record.Kind,
			Pinned:   record.Pinned,
			StartsAt: record.StartsAt,
			EndsAt:   record.EndsAt,
		})
	}
	sortDesktopAnnouncementItems(items)

	release := latestPublishedDesktopRelease(feed.Releases, platform, arch)
	if release == nil {
		return items, nil
	}
	return append(items, release.AnnouncementItems...), nil
}

func (s *DesktopUpdateService) ServePackage(ctx context.Context, releaseID int64) (string, string, error) {
	release, err := s.GetPublishedRelease(ctx, releaseID)
	if err != nil {
		return "", "", err
	}

	path := filepath.Join(s.releasesRoot(), release.ReleaseSlug, release.FileName)
	if _, err := os.Stat(path); err != nil {
		if os.IsNotExist(err) {
			return "", "", ErrDesktopReleaseNotFound
		}
		return "", "", fmt.Errorf("stat desktop release package: %w", err)
	}

	contentType := mime.TypeByExtension(filepath.Ext(release.FileName))
	if contentType == "" {
		contentType = "application/octet-stream"
	}
	return contentType, path, nil
}

func latestPublishedDesktopRelease(releases []DesktopReleaseRecord, platform, arch string) *DesktopReleaseRecord {
	var best *DesktopReleaseRecord
	normalizedPlatform := strings.TrimSpace(platform)
	normalizedArch := strings.TrimSpace(arch)

	for i := range releases {
		release := &releases[i]
		if !release.Published {
			continue
		}
		if release.Platform != normalizedPlatform || release.Arch != normalizedArch {
			continue
		}
		if best == nil || compareVersions(best.Version, release.Version) < 0 {
			best = release
		}
	}
	return best
}

func desktopReleaseIndexByID(releases []DesktopReleaseRecord, releaseID int64) int {
	for i := range releases {
		if releases[i].ID == releaseID {
			return i
		}
	}
	return -1
}

func sortDesktopReleases(releases []DesktopReleaseRecord) {
	for i := 0; i < len(releases); i++ {
		for j := i + 1; j < len(releases); j++ {
			if compareVersions(releases[i].Version, releases[j].Version) < 0 {
				releases[i], releases[j] = releases[j], releases[i]
			}
		}
	}
}

func desktopStandaloneAnnouncementIndexByID(items []DesktopStandaloneAnnouncementRecord, announcementID int64) int {
	for i := range items {
		if items[i].ID == announcementID {
			return i
		}
	}
	return -1
}

func sortDesktopStandaloneAnnouncements(items []DesktopStandaloneAnnouncementRecord) {
	for i := 0; i < len(items); i++ {
		for j := i + 1; j < len(items); j++ {
			if shouldSwapStandaloneAnnouncements(items[i], items[j]) {
				items[i], items[j] = items[j], items[i]
			}
		}
	}
}

func sortDesktopAnnouncementItems(items []DesktopAnnouncementItem) {
	for i := 0; i < len(items); i++ {
		for j := i + 1; j < len(items); j++ {
			if !items[i].Pinned && items[j].Pinned {
				items[i], items[j] = items[j], items[i]
			}
		}
	}
}

func shouldSwapStandaloneAnnouncements(left, right DesktopStandaloneAnnouncementRecord) bool {
	if left.Pinned != right.Pinned {
		return !left.Pinned && right.Pinned
	}
	return left.CreatedAt.Before(right.CreatedAt)
}

func isStandaloneAnnouncementActive(record DesktopStandaloneAnnouncementRecord, now time.Time) bool {
	if record.StartsAt != nil && record.StartsAt.After(now) {
		return false
	}
	if record.EndsAt != nil && !record.EndsAt.After(now) {
		return false
	}
	return true
}

func buildDesktopReleaseSlug(id int64, version, platform, arch string) string {
	versionSlug := strings.NewReplacer(".", "-", " ", "-", "/", "-").Replace(strings.TrimSpace(version))
	platformSlug := strings.NewReplacer(" ", "-", "/", "-").Replace(strings.TrimSpace(platform))
	archSlug := strings.NewReplacer(" ", "-", "/", "-").Replace(strings.TrimSpace(arch))
	return fmt.Sprintf("%s-%s-%s-%d", platformSlug, archSlug, versionSlug, id)
}

func copyDesktopReleaseFile(src, dest string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()

	out, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer func() { _ = out.Close() }()

	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	return out.Close()
}

func fileSHA256(path string) (string, int64, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", 0, err
	}
	defer func() { _ = f.Close() }()

	h := sha256.New()
	n, err := io.Copy(h, f)
	if err != nil {
		return "", 0, err
	}
	return hex.EncodeToString(h.Sum(nil)), n, nil
}
