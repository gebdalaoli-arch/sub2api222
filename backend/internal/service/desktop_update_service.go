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
	"strings"
	"time"
)

type DesktopUpdateService struct {
	settingRepo SettingRepository
	rootDir     string
}

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
	if feed.Releases == nil {
		feed.Releases = []DesktopReleaseRecord{}
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
