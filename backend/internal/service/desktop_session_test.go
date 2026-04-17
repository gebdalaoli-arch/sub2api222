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
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return currentNow },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
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
	require.Equal(t, int64(9), repo.mustGet(created.SessionID).GroupID)

	currentNow = baseNow.Add(2 * time.Hour)
	refreshed, err := svc.Refresh(context.Background(), created.SessionID, 42)
	require.NoError(t, err)
	require.Equal(t, currentNow.Add(12*time.Hour), refreshed.ExpiresAt)

	require.NoError(t, svc.Revoke(context.Background(), created.SessionID, 42))
	stored := repo.mustGet(created.SessionID)
	require.NotNil(t, stored.RevokedAt)
	require.Equal(t, "revoked", stored.Status)
}

func TestDesktopSessionService_CreateStoresRuntimeTokenHashAndValidateRuntimeToken(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionSubscriptionGroup(9)},
		&desktopSessionSubscriptionReaderStub{sub: &UserSubscription{ID: 100, UserID: 42, GroupID: 9, Status: SubscriptionStatusActive, ExpiresAt: now.Add(time.Hour)}},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
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

func TestDesktopSessionService_CreateRejectsSubscriptionGroupWithoutActiveSubscription(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionSubscriptionGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.Nil(t, created)
	require.Error(t, err)
	require.True(t, infraerrors.IsBadRequest(err))
}

func TestDesktopSessionService_ValidateRuntimeTokenRejectsExpiredSession(t *testing.T) {
	baseNow := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	currentNow := baseNow
	repo := newDesktopSessionRepoStub(baseNow)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return currentNow },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
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

func TestDesktopSessionService_ValidateRuntimeTokenRejectsRevokedSession(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)
	require.NoError(t, svc.Revoke(context.Background(), created.SessionID, 42))

	validator, ok := any(svc).(desktopRuntimeTokenValidator)
	require.True(t, ok, "DesktopSessionService must expose ValidateRuntimeToken")

	validated, err := validator.ValidateRuntimeToken(context.Background(), created.RuntimeToken)
	require.Nil(t, validated)
	require.Error(t, err)
	require.True(t, infraerrors.IsUnauthorized(err))
}

func TestDesktopSessionService_ValidateRuntimeTokenRejectsUnknownToken(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	validator, ok := any(svc).(desktopRuntimeTokenValidator)
	require.True(t, ok, "DesktopSessionService must expose ValidateRuntimeToken")

	validated, err := validator.ValidateRuntimeToken(context.Background(), "missing-token")
	require.Nil(t, validated)
	require.Error(t, err)
	require.True(t, infraerrors.IsUnauthorized(err))
	require.Equal(t, hashDesktopRuntimeToken("missing-token"), repo.lastRuntimeTokenHashLookup)
	require.Equal(t, 1, repo.getByRuntimeTokenHashCalls)
}

func TestDesktopSessionService_RefreshRejectsRevokedSession(t *testing.T) {
	baseNow := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	currentNow := baseNow
	repo := newDesktopSessionRepoStub(baseNow)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return currentNow },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)
	require.NoError(t, svc.Revoke(context.Background(), created.SessionID, 42))

	currentNow = baseNow.Add(30 * time.Minute)
	refreshed, err := svc.Refresh(context.Background(), created.SessionID, 42)
	require.Nil(t, refreshed)
	require.Error(t, err)
	require.True(t, infraerrors.IsUnauthorized(err))

	stored := repo.mustGet(created.SessionID)
	require.Equal(t, created.ExpiresAt, stored.ExpiresAt)
	require.Equal(t, "revoked", stored.Status)
}

func TestDesktopSessionService_RefreshRejectsExpiredSession(t *testing.T) {
	baseNow := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	currentNow := baseNow
	repo := newDesktopSessionRepoStub(baseNow)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return currentNow },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)

	currentNow = created.ExpiresAt
	refreshed, err := svc.Refresh(context.Background(), created.SessionID, 42)
	require.Nil(t, refreshed)
	require.Error(t, err)
	require.True(t, infraerrors.IsUnauthorized(err))

	stored := repo.mustGet(created.SessionID)
	require.Equal(t, created.ExpiresAt, stored.ExpiresAt)
	require.Equal(t, "active", stored.Status)
}

func TestDesktopSessionService_RefreshRejectsNonOwner(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)

	refreshed, err := svc.Refresh(context.Background(), created.SessionID, 99)
	require.Nil(t, refreshed)
	require.Error(t, err)
	require.True(t, infraerrors.IsNotFound(err))
}

func TestDesktopSessionService_RevokeRejectsNonOwner(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42, AllowedGroups: []int64{9}}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.NoError(t, err)

	err = svc.Revoke(context.Background(), created.SessionID, 99)
	require.Error(t, err)
	require.True(t, infraerrors.IsNotFound(err))
	require.Nil(t, repo.mustGet(created.SessionID).RevokedAt)
}

func TestDesktopSessionService_CreateRejectsGroupWithoutBindingPermission(t *testing.T) {
	now := time.Date(2026, 4, 18, 9, 0, 0, 0, time.UTC)
	repo := newDesktopSessionRepoStub(now)
	svc := NewDesktopSessionService(
		repo,
		&desktopSessionUserReaderStub{user: &User{ID: 42}},
		&desktopSessionGroupReaderStub{group: newDesktopSessionTestGroup(9)},
		&desktopSessionSubscriptionReaderStub{},
		func() time.Time { return now },
		[]byte("desktop-test-secret"),
	)

	created, err := svc.Create(context.Background(), DesktopSessionCreateRequest{
		UserID:        42,
		GroupID:       9,
		DeviceID:      "device-001",
		DeviceName:    "MacBook Pro",
		Target:        DesktopSessionTargetDesktop,
		ClientVersion: "0.1.0",
	})
	require.Nil(t, created)
	require.Error(t, err)
	require.True(t, infraerrors.IsForbidden(err))
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

func (s *desktopSessionRepoStub) GetBySessionIDAndUserID(_ context.Context, sessionID string, userID int64) (*DesktopSession, error) {
	session, ok := s.records[sessionID]
	if !ok || session.UserID != userID {
		return nil, nil
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

func (s *desktopSessionRepoStub) Revoke(_ context.Context, sessionID string, userID int64, revokedAt time.Time) error {
	session, ok := s.records[sessionID]
	if !ok || session.UserID != userID {
		return ErrDesktopSessionNotFound
	}
	revokedAtCopy := revokedAt
	session.RevokedAt = &revokedAtCopy
	session.Status = "revoked"
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
	return nil, nil
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

type desktopSessionUserReaderStub struct {
	user *User
	err  error
}

func (s *desktopSessionUserReaderStub) GetByID(_ context.Context, id int64) (*User, error) {
	if s.err != nil {
		return nil, s.err
	}
	if s.user != nil {
		clone := *s.user
		return &clone, nil
	}
	return &User{ID: id}, nil
}

type desktopSessionGroupReaderStub struct {
	group *Group
	err   error
}

func (s *desktopSessionGroupReaderStub) GetByID(_ context.Context, id int64) (*Group, error) {
	if s.err != nil {
		return nil, s.err
	}
	if s.group != nil {
		clone := *s.group
		clone.ID = id
		return &clone, nil
	}
	return newDesktopSessionTestGroup(id), nil
}

type desktopSessionSubscriptionReaderStub struct {
	sub *UserSubscription
	err error
}

func (s *desktopSessionSubscriptionReaderStub) GetActiveByUserIDAndGroupID(_ context.Context, userID, groupID int64) (*UserSubscription, error) {
	if s.err != nil {
		return nil, s.err
	}
	if s.sub != nil {
		clone := *s.sub
		clone.UserID = userID
		clone.GroupID = groupID
		return &clone, nil
	}
	return nil, ErrSubscriptionNotFound
}

func newDesktopSessionSubscriptionGroup(groupID int64) *Group {
	group := newDesktopSessionTestGroup(groupID)
	group.SubscriptionType = SubscriptionTypeSubscription
	group.IsExclusive = false
	return group
}

func newDesktopSessionTestGroup(groupID int64) *Group {
	return &Group{
		ID:               groupID,
		Platform:         PlatformOpenAI,
		Status:           StatusActive,
		Hydrated:         true,
		IsExclusive:      true,
		SubscriptionType: SubscriptionTypeStandard,
	}
}
