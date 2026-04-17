package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"time"

	"github.com/google/uuid"
)

type DesktopSessionTarget string

const (
	DesktopSessionTargetDesktop DesktopSessionTarget = "desktop"
	DesktopSessionTargetCLI     DesktopSessionTarget = "cli"
)

type DesktopSession struct {
	ID               int64
	SessionID        string
	UserID           int64
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
	Update(ctx context.Context, session *DesktopSession) error
	Revoke(ctx context.Context, sessionID string, revokedAt time.Time) error
}

type DesktopSessionCreateRequest struct {
	UserID        int64
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
	repo       DesktopSessionRepository
	now        func() time.Time
	signingKey []byte
}

func NewDesktopSessionService(repo DesktopSessionRepository, now func() time.Time, signingKey []byte) *DesktopSessionService {
	return &DesktopSessionService{repo: repo, now: now, signingKey: signingKey}
}

func (s *DesktopSessionService) Create(ctx context.Context, req DesktopSessionCreateRequest) (*DesktopSessionResult, error) {
	now := s.now()
	sessionID := uuid.NewString()
	token := uuid.NewString() + "." + uuid.NewString()
	expiresAt := now.Add(12 * time.Hour)
	record := &DesktopSession{
		SessionID:        sessionID,
		UserID:           req.UserID,
		DeviceID:         req.DeviceID,
		DeviceName:       req.DeviceName,
		Target:           string(req.Target),
		Status:           "active",
		RuntimeTokenHash: hashDesktopRuntimeToken(token),
		ProfileKey:       "platform-" + string(req.Target),
		ExpiresAt:        expiresAt,
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
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      expiresAt,
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *DesktopSessionService) Refresh(ctx context.Context, sessionID string) (*DesktopSessionResult, error) {
	record, err := s.repo.GetBySessionID(ctx, sessionID)
	if err != nil {
		return nil, err
	}
	now := s.now()
	base := now
	if record.ExpiresAt.After(base) {
		base = record.ExpiresAt
	}
	record.ExpiresAt = base.Add(12 * time.Hour)
	record.LastSeenAt = now
	if err := s.repo.Update(ctx, record); err != nil {
		return nil, err
	}
	return &DesktopSessionResult{
		SessionID:      record.SessionID,
		UserID:         record.UserID,
		ProfileKey:     record.ProfileKey,
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      record.ExpiresAt,
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *DesktopSessionService) Revoke(ctx context.Context, sessionID string) error {
	return s.repo.Revoke(ctx, sessionID, s.now())
}

func hashDesktopRuntimeToken(token string) string {
	sum := sha256.Sum256([]byte(token))
	return hex.EncodeToString(sum[:])
}
