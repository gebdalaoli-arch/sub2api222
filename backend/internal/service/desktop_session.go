package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"time"

	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
	"github.com/google/uuid"
)

type DesktopSessionTarget string

const (
	DesktopSessionTargetDesktop DesktopSessionTarget = "desktop"
	DesktopSessionTargetCLI     DesktopSessionTarget = "cli"

	desktopSessionStatusActive  = "active"
	desktopSessionStatusRevoked = "revoked"

	desktopSessionTTL          = 12 * time.Hour
	desktopSessionRefreshAfter = 30 * time.Minute
	desktopSessionGatewayPath  = "/api/desktop/v1"
)

type DesktopSession struct {
	ID               int64
	SessionID        string
	UserID           int64
	GroupID          int64
	DeviceID         string
	DeviceName       string
	Target           string
	Status           string
	RuntimeTokenHash string
	ProfileKey       string
	ExpiresAt        time.Time
	LastSeenAt       time.Time
	RevokedAt        *time.Time
	CreatedAt        time.Time
	UpdatedAt        time.Time
}

type DesktopSessionRepository interface {
	Create(ctx context.Context, session *DesktopSession) error
	GetBySessionID(ctx context.Context, sessionID string) (*DesktopSession, error)
	GetBySessionIDAndUserID(ctx context.Context, sessionID string, userID int64) (*DesktopSession, error)
	GetByRuntimeTokenHash(ctx context.Context, tokenHash string) (*DesktopSession, error)
	Update(ctx context.Context, session *DesktopSession) error
	Revoke(ctx context.Context, sessionID string, userID int64, revokedAt time.Time) error
}

type desktopSessionUserReader interface {
	GetByID(ctx context.Context, id int64) (*User, error)
}

type desktopSessionGroupReader interface {
	GetByID(ctx context.Context, id int64) (*Group, error)
}

type desktopSessionSubscriptionReader interface {
	GetActiveByUserIDAndGroupID(ctx context.Context, userID, groupID int64) (*UserSubscription, error)
}

var (
	ErrInvalidDesktopRuntimeToken       = infraerrors.Unauthorized("INVALID_RUNTIME_TOKEN", "invalid runtime token")
	ErrInactiveDesktopSession           = infraerrors.Unauthorized("DESKTOP_SESSION_INACTIVE", "desktop session is inactive")
	ErrDesktopSessionNotFound           = infraerrors.NotFound("DESKTOP_SESSION_NOT_FOUND", "desktop session not found")
	ErrDesktopSessionGroupRequired      = infraerrors.BadRequest("DESKTOP_SESSION_GROUP_REQUIRED", "group_id is required")
	ErrDesktopSessionGroupForbidden     = infraerrors.Forbidden("DESKTOP_SESSION_GROUP_FORBIDDEN", "user cannot start desktop session for this group")
	ErrDesktopSessionSubscriptionNeeded = infraerrors.BadRequest("SUBSCRIPTION_REQUIRED", "user does not have an active subscription for this group")
)

type DesktopSessionCreateRequest struct {
	UserID        int64
	GroupID       int64
	DeviceID      string
	DeviceName    string
	Target        DesktopSessionTarget
	ClientVersion string
}

type DesktopSessionResult struct {
	SessionID      string
	UserID         int64
	RuntimeToken   string
	ProfileKey     string
	RefreshAfter   time.Duration
	ExpiresAt      time.Time
	GatewayBaseURL string
}

type DesktopSessionService struct {
	repo          DesktopSessionRepository
	users         desktopSessionUserReader
	groups        desktopSessionGroupReader
	subscriptions desktopSessionSubscriptionReader
	now           func() time.Time
	signingKey    []byte
}

func NewDesktopSessionService(
	repo DesktopSessionRepository,
	users desktopSessionUserReader,
	groups desktopSessionGroupReader,
	subscriptions desktopSessionSubscriptionReader,
	now func() time.Time,
	signingKey []byte,
) *DesktopSessionService {
	return &DesktopSessionService{
		repo:          repo,
		users:         users,
		groups:        groups,
		subscriptions: subscriptions,
		now:           now,
		signingKey:    signingKey,
	}
}

func (s *DesktopSessionService) Create(ctx context.Context, req DesktopSessionCreateRequest) (*DesktopSessionResult, error) {
	group, err := s.resolveAccessibleGroup(ctx, req.UserID, req.GroupID)
	if err != nil {
		return nil, err
	}

	now := s.now()
	sessionID := uuid.NewString()
	token := uuid.NewString() + "." + uuid.NewString()
	record := &DesktopSession{
		SessionID:        sessionID,
		UserID:           req.UserID,
		GroupID:          group.ID,
		DeviceID:         req.DeviceID,
		DeviceName:       req.DeviceName,
		Target:           string(req.Target),
		Status:           desktopSessionStatusActive,
		RuntimeTokenHash: hashDesktopRuntimeToken(token),
		ProfileKey:       "platform-" + string(req.Target),
		ExpiresAt:        now.Add(desktopSessionTTL),
		LastSeenAt:       now,
	}
	if err := s.repo.Create(ctx, record); err != nil {
		return nil, err
	}
	return &DesktopSessionResult{
		SessionID:      sessionID,
		UserID:         req.UserID,
		RuntimeToken:   token,
		ProfileKey:     record.ProfileKey,
		RefreshAfter:   desktopSessionRefreshAfter,
		ExpiresAt:      record.ExpiresAt,
		GatewayBaseURL: desktopSessionGatewayPath,
	}, nil
}

func (s *DesktopSessionService) Refresh(ctx context.Context, sessionID string, userID int64) (*DesktopSessionResult, error) {
	record, err := s.repo.GetBySessionIDAndUserID(ctx, sessionID, userID)
	if err != nil {
		return nil, err
	}
	now := s.now()
	if !isDesktopSessionActive(record, now) {
		if record == nil {
			return nil, ErrDesktopSessionNotFound
		}
		return nil, ErrInactiveDesktopSession
	}
	record.ExpiresAt = now.Add(desktopSessionTTL)
	record.LastSeenAt = now
	if err := s.repo.Update(ctx, record); err != nil {
		return nil, err
	}
	return &DesktopSessionResult{
		SessionID:      record.SessionID,
		UserID:         record.UserID,
		ProfileKey:     record.ProfileKey,
		RefreshAfter:   desktopSessionRefreshAfter,
		ExpiresAt:      record.ExpiresAt,
		GatewayBaseURL: desktopSessionGatewayPath,
	}, nil
}

func (s *DesktopSessionService) Revoke(ctx context.Context, sessionID string, userID int64) error {
	return s.repo.Revoke(ctx, sessionID, userID, s.now())
}

func (s *DesktopSessionService) ValidateRuntimeToken(ctx context.Context, token string) (*DesktopSession, error) {
	record, err := s.repo.GetByRuntimeTokenHash(ctx, hashDesktopRuntimeToken(token))
	if err != nil {
		return nil, err
	}
	if !isDesktopSessionActive(record, s.now()) {
		return nil, ErrInvalidDesktopRuntimeToken
	}
	return record, nil
}

func (s *DesktopSessionService) resolveAccessibleGroup(ctx context.Context, userID, groupID int64) (*Group, error) {
	if groupID <= 0 {
		return nil, ErrDesktopSessionGroupRequired
	}
	if s.users == nil {
		return nil, ErrUserNotFound
	}
	user, err := s.users.GetByID(ctx, userID)
	if err != nil {
		return nil, err
	}
	if user == nil {
		return nil, ErrUserNotFound
	}
	if s.groups == nil {
		return nil, ErrGroupNotFound
	}
	group, err := s.groups.GetByID(ctx, groupID)
	if err != nil {
		return nil, err
	}
	if !IsGroupContextValid(group) {
		return nil, ErrGroupNotFound
	}

	if group.IsSubscriptionType() {
		if s.subscriptions == nil {
			return nil, ErrDesktopSessionSubscriptionNeeded
		}
		subscription, subErr := s.subscriptions.GetActiveByUserIDAndGroupID(ctx, user.ID, group.ID)
		if subErr != nil {
			if errors.Is(subErr, ErrSubscriptionNotFound) {
				return nil, ErrDesktopSessionSubscriptionNeeded
			}
			return nil, subErr
		}
		if subscription == nil {
			return nil, ErrDesktopSessionSubscriptionNeeded
		}
		return group, nil
	}

	if !user.CanBindGroup(group.ID, group.IsExclusive) {
		return nil, ErrDesktopSessionGroupForbidden
	}
	return group, nil
}

func isDesktopSessionActive(record *DesktopSession, now time.Time) bool {
	if record == nil {
		return false
	}
	if record.Status != desktopSessionStatusActive {
		return false
	}
	if record.RevokedAt != nil {
		return false
	}
	return record.ExpiresAt.After(now)
}

func hashDesktopRuntimeToken(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}
