package repository

import (
	"context"
	"database/sql"
	"testing"
	"time"

	dbent "github.com/Wei-Shaw/sub2api/ent"
	"github.com/Wei-Shaw/sub2api/ent/enttest"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/stretchr/testify/require"

	"entgo.io/ent/dialect"
	entsql "entgo.io/ent/dialect/sql"
	_ "modernc.org/sqlite"
)

func newDesktopSessionEntRepo(t *testing.T) (service.DesktopSessionRepository, *dbent.Client) {
	t.Helper()

	db, err := sql.Open("sqlite", "file:desktop_session_repo?mode=memory&cache=shared")
	require.NoError(t, err)
	t.Cleanup(func() { _ = db.Close() })

	_, err = db.Exec("PRAGMA foreign_keys = ON")
	require.NoError(t, err)

	drv := entsql.OpenDB(dialect.SQLite, db)
	client := enttest.NewClient(t, enttest.WithOptions(dbent.Driver(drv)))
	t.Cleanup(func() { _ = client.Close() })

	return NewDesktopSessionRepository(client), client
}

func TestDesktopSessionRepository_CreateLookupAndRevoke(t *testing.T) {
	repo, _ := newDesktopSessionEntRepo(t)
	ctx := context.Background()
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	expiresAt := now.Add(12 * time.Hour)

	record := &service.DesktopSession{
		SessionID:        "session-001",
		UserID:           42,
		GroupID:          88,
		DeviceID:         "device-001",
		DeviceName:       "MacBook Pro",
		Target:           "desktop",
		Status:           "active",
		RuntimeTokenHash: "hash-001",
		ProfileKey:       "platform-desktop",
		ExpiresAt:        expiresAt,
		LastSeenAt:       now,
	}
	require.NoError(t, repo.Create(ctx, record))
	require.NotZero(t, record.ID)

	bySessionID, err := repo.GetBySessionID(ctx, record.SessionID)
	require.NoError(t, err)
	require.NotNil(t, bySessionID)
	require.Equal(t, record.RuntimeTokenHash, bySessionID.RuntimeTokenHash)
	require.Equal(t, int64(88), bySessionID.GroupID)

	owned, err := repo.GetBySessionIDAndUserID(ctx, record.SessionID, 42)
	require.NoError(t, err)
	require.NotNil(t, owned)
	require.Equal(t, int64(88), owned.GroupID)

	notOwned, err := repo.GetBySessionIDAndUserID(ctx, record.SessionID, 99)
	require.NoError(t, err)
	require.Nil(t, notOwned)

	byTokenHash, err := repo.GetByRuntimeTokenHash(ctx, record.RuntimeTokenHash)
	require.NoError(t, err)
	require.NotNil(t, byTokenHash)
	require.Equal(t, record.SessionID, byTokenHash.SessionID)

	revokedAt := now.Add(time.Hour)
	require.NoError(t, repo.Revoke(ctx, record.SessionID, 42, revokedAt))

	revoked, err := repo.GetBySessionID(ctx, record.SessionID)
	require.NoError(t, err)
	require.NotNil(t, revoked)
	require.NotNil(t, revoked.RevokedAt)
	require.Equal(t, "revoked", revoked.Status)
	require.WithinDuration(t, revokedAt, *revoked.RevokedAt, time.Second)
}

func TestDesktopSessionRepository_RuntimeTokenHashMustBeUnique(t *testing.T) {
	repo, _ := newDesktopSessionEntRepo(t)
	ctx := context.Background()
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)

	first := &service.DesktopSession{
		SessionID:        "session-001",
		UserID:           42,
		GroupID:          88,
		DeviceID:         "device-001",
		DeviceName:       "MacBook Pro",
		Target:           "desktop",
		Status:           "active",
		RuntimeTokenHash: "shared-hash",
		ProfileKey:       "platform-desktop",
		ExpiresAt:        now.Add(12 * time.Hour),
		LastSeenAt:       now,
	}
	second := &service.DesktopSession{
		SessionID:        "session-002",
		UserID:           43,
		GroupID:          89,
		DeviceID:         "device-002",
		DeviceName:       "Mac mini",
		Target:           "desktop",
		Status:           "active",
		RuntimeTokenHash: "shared-hash",
		ProfileKey:       "platform-desktop",
		ExpiresAt:        now.Add(12 * time.Hour),
		LastSeenAt:       now,
	}

	require.NoError(t, repo.Create(ctx, first))
	err := repo.Create(ctx, second)
	require.Error(t, err)
}

func TestDesktopSessionRepository_UpdateMissingSessionReturnsError(t *testing.T) {
	repo, _ := newDesktopSessionEntRepo(t)
	ctx := context.Background()
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)

	record := &service.DesktopSession{
		ID:               999,
		SessionID:        "missing-session",
		UserID:           42,
		GroupID:          88,
		DeviceID:         "device-001",
		DeviceName:       "MacBook Pro",
		Target:           "desktop",
		Status:           "active",
		RuntimeTokenHash: "hash-missing",
		ProfileKey:       "platform-desktop",
		ExpiresAt:        now.Add(12 * time.Hour),
		LastSeenAt:       now,
	}

	err := repo.Update(ctx, record)
	require.Error(t, err)
}

func TestDesktopSessionRepository_RevokeMissingSessionReturnsError(t *testing.T) {
	repo, _ := newDesktopSessionEntRepo(t)
	ctx := context.Background()

	err := repo.Revoke(ctx, "missing-session", 42, time.Date(2026, 4, 18, 10, 0, 0, 0, time.UTC))
	require.Error(t, err)
}

func TestDesktopSessionRepository_RevokeRejectsNonOwner(t *testing.T) {
	repo, _ := newDesktopSessionEntRepo(t)
	ctx := context.Background()
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)

	record := &service.DesktopSession{
		SessionID:        "session-001",
		UserID:           42,
		GroupID:          88,
		DeviceID:         "device-001",
		DeviceName:       "MacBook Pro",
		Target:           "desktop",
		Status:           "active",
		RuntimeTokenHash: "hash-001",
		ProfileKey:       "platform-desktop",
		ExpiresAt:        now.Add(12 * time.Hour),
		LastSeenAt:       now,
	}
	require.NoError(t, repo.Create(ctx, record))

	err := repo.Revoke(ctx, record.SessionID, 99, now.Add(time.Hour))
	require.Error(t, err)

	stored, getErr := repo.GetBySessionID(ctx, record.SessionID)
	require.NoError(t, getErr)
	require.NotNil(t, stored)
	require.Nil(t, stored.RevokedAt)
}
