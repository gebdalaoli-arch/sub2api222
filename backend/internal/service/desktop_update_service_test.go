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
		Version:              "0.2.0",
		Platform:             "windows",
		Arch:                 "x64",
		Title:                "发现新版本",
		Summary:              "修复若干问题",
		ReleaseNotesMarkdown: "## 新增\n- 更稳定",
		AnnouncementItems: []DesktopAnnouncementItem{{
			Title:   "维护提醒",
			Content: "本周五维护",
			Kind:    "maintenance",
		}},
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
