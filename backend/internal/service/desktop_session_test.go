package service

import (
	"context"
	"errors"
	"testing"
	"time"

	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
	"github.com/stretchr/testify/require"
)

func TestDesktopSessionService_CreateRefreshRevoke(t *testing.T) {
	baseNow := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	currentNow := baseNow
	repo := newDesktopSessionRepoStub(baseNow)
	svc := NewDesktopSessionService(repo, func() time.Time { return currentNow }, []byte("desktop-test-secret"))

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)
	require.NotEmpty(t, created.SessionID)
	require.NotEmpty(t, created.RuntimeToken)
	require.Equal(t, int64(42), created.UserID)
	require.Equal(t, baseNow.Add(12*time.Hour), created.ExpiresAt)

	currentNow = baseNow.Add(2 * time.Hour)
	refreshed, err := svc.Refresh(context.Background(), created.SessionID)
	require.NoError(t, err)
	require.Equal(t, currentNow.Add(12*time.Hour), refreshed.ExpiresAt)

	require.NoError(t, svc.Revoke(context.Background(), created.SessionID))
	stored := repo.mustGet(created.SessionID)
	require.NotNil(t, stored.RevokedAt)
}

func TestDesktopSessionService_CreateStoresRuntimeTokenHashAndValidateRuntimeToken(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(repo, func() time.Time { return now }, []byte("desktop-test-secret"))

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)

	stored := repo.mustGet(created.SessionID)
	require.Equal(t, hashDesktopRuntimeToken(created.RuntimeToken), stored.RuntimeTokenHash)

	validator, ok := any(svc).(desktopRuntimeTokenValidator)
	require.True(t, ok, "DesktopSessionService must expose ValidateRuntimeToken")

	validated, err := validator.ValidateRuntimeToken(context.Background(), created.RuntimeToken)
	require.NoError(t, err)
	require.Equal(t, created.SessionID, validated.SessionID)
	require.Equal(t, stored.RuntimeTokenHash, repo.lastRuntimeTokenHashLookup)
	require.Equal(t, 1, repo.getByRuntimeTokenHashCalls)
}

func TestDesktopSessionService_ValidateRuntimeTokenRejectsExpiredSession(t *testing.T) {
	baseNow := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	currentNow := baseNow
	repo := newDesktopSessionRepoStub(baseNow)
	svc := NewDesktopSessionService(repo, func() time.Time { return currentNow }, []byte("desktop-test-secret"))

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)

	validator, ok := any(svc).(desktopRuntimeTokenValidator)
	require.True(t, ok, "DesktopSessionService must expose ValidateRuntimeToken")

	currentNow = baseNow.Add(12 * time.Hour)
	validated, err := validator.ValidateRuntimeToken(context.Background(), created.RuntimeToken)
	require.Nil(t, validated)
	require.Error(t, err)
	require.True(t, infraerrors.IsUnauthorized(err))
}

type desktopRuntimeTokenValidator interface {
	ValidateRuntimeToken(ctx context.Context, token string) (*DesktopSession, error)
}

type desktopSessionRepoStub struct {
	now                        time.Time
	records                    map[string]*DesktopSession
	getByRuntimeTokenHashCalls int
	lastRuntimeTokenHashLookup string
}

func newDesktopSessionRepoStub(now time.Time) *desktopSessionRepoStub {
	return &desktopSessionRepoStub{
		now:     now,
		records: make(map[string]*DesktopSession),
	}
}

func (s *desktopSessionRepoStub) Create(_ context.Context, session *DesktopSession) error {
	s.records[session.SessionID] = cloneDesktopSession(session)
	return nil
}

func (s *desktopSessionRepoStub) GetBySessionID(_ context.Context, sessionID string) (*DesktopSession, error) {
	session, ok := s.records[sessionID]
	if !ok {
		return nil, errors.New("desktop session not found")
	}
	return cloneDesktopSession(session), nil
}

func (s *desktopSessionRepoStub) Update(_ context.Context, session *DesktopSession) error {
	if _, ok := s.records[session.SessionID]; !ok {
		return errors.New("desktop session not found")
	}
	s.records[session.SessionID] = cloneDesktopSession(session)
	return nil
}

func (s *desktopSessionRepoStub) Revoke(_ context.Context, sessionID string, revokedAt time.Time) error {
	session, ok := s.records[sessionID]
	if !ok {
		return errors.New("desktop session not found")
	}
	revokedAtCopy := revokedAt
	session.RevokedAt = &revokedAtCopy
	s.records[sessionID] = cloneDesktopSession(session)
	return nil
}

func (s *desktopSessionRepoStub) GetByRuntimeTokenHash(_ context.Context, tokenHash string) (*DesktopSession, error) {
	s.getByRuntimeTokenHashCalls++
	s.lastRuntimeTokenHashLookup = tokenHash
	for _, session := range s.records {
		if session.RuntimeTokenHash == tokenHash {
			return cloneDesktopSession(session), nil
		}
	}
	return nil, errors.New("desktop session not found")
}

func (s *desktopSessionRepoStub) mustGet(sessionID string) *DesktopSession {
	session, ok := s.records[sessionID]
	if !ok {
		panic("desktop session not found")
	}
	return cloneDesktopSession(session)
}

func cloneDesktopSession(session *DesktopSession) *DesktopSession {
	if session == nil {
		return nil
	}
	copy := *session
	if session.RevokedAt != nil {
		revokedAtCopy := *session.RevokedAt
		copy.RevokedAt = &revokedAtCopy
	}
	return &copy
}
