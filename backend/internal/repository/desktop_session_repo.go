package repository

import (
	"context"
	"time"

	dbent "github.com/Wei-Shaw/sub2api/ent"
	dbdesktopsession "github.com/Wei-Shaw/sub2api/ent/desktopsession"
	"github.com/Wei-Shaw/sub2api/internal/service"
)

type desktopSessionRepository struct {
	client *dbent.Client
}

func NewDesktopSessionRepository(client *dbent.Client) service.DesktopSessionRepository {
	return &desktopSessionRepository{client: client}
}

func (r *desktopSessionRepository) Create(ctx context.Context, session *service.DesktopSession) error {
	if session == nil {
		return nil
	}

	client := clientFromContext(ctx, r.client)
	builder := client.DesktopSession.Create().
		SetSessionID(session.SessionID).
		SetUserID(session.UserID).
		SetGroupID(session.GroupID).
		SetDeviceID(session.DeviceID).
		SetDeviceName(session.DeviceName).
		SetTarget(session.Target).
		SetStatus(session.Status).
		SetRuntimeTokenHash(session.RuntimeTokenHash).
		SetProfileKey(session.ProfileKey).
		SetExpiresAt(session.ExpiresAt).
		SetLastSeenAt(session.LastSeenAt)
	if session.RevokedAt != nil {
		builder.SetRevokedAt(*session.RevokedAt)
	}

	created, err := builder.Save(ctx)
	if err != nil {
		return err
	}
	applyDesktopSessionEntity(session, created)
	return nil
}

func (r *desktopSessionRepository) GetBySessionID(ctx context.Context, sessionID string) (*service.DesktopSession, error) {
	client := clientFromContext(ctx, r.client)
	record, err := client.DesktopSession.Query().
		Where(dbdesktopsession.SessionIDEQ(sessionID)).
		Only(ctx)
	if err != nil {
		if dbent.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}
	return desktopSessionEntityToService(record), nil
}

func (r *desktopSessionRepository) GetBySessionIDAndUserID(ctx context.Context, sessionID string, userID int64) (*service.DesktopSession, error) {
	client := clientFromContext(ctx, r.client)
	record, err := client.DesktopSession.Query().
		Where(
			dbdesktopsession.SessionIDEQ(sessionID),
			dbdesktopsession.UserIDEQ(userID),
		).
		Only(ctx)
	if err != nil {
		if dbent.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}
	return desktopSessionEntityToService(record), nil
}

func (r *desktopSessionRepository) GetByRuntimeTokenHash(ctx context.Context, tokenHash string) (*service.DesktopSession, error) {
	client := clientFromContext(ctx, r.client)
	record, err := client.DesktopSession.Query().
		Where(dbdesktopsession.RuntimeTokenHashEQ(tokenHash)).
		Only(ctx)
	if err != nil {
		if dbent.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}
	return desktopSessionEntityToService(record), nil
}

func (r *desktopSessionRepository) Update(ctx context.Context, session *service.DesktopSession) error {
	if session == nil {
		return nil
	}

	client := clientFromContext(ctx, r.client)
	record, err := resolveDesktopSessionEntity(ctx, client, session.ID, session.SessionID)
	if err != nil {
		return err
	}
	builder := record.Update().
		SetUserID(session.UserID).
		SetGroupID(session.GroupID).
		SetDeviceID(session.DeviceID).
		SetDeviceName(session.DeviceName).
		SetTarget(session.Target).
		SetStatus(session.Status).
		SetRuntimeTokenHash(session.RuntimeTokenHash).
		SetProfileKey(session.ProfileKey).
		SetExpiresAt(session.ExpiresAt).
		SetLastSeenAt(session.LastSeenAt)
	if session.RevokedAt != nil {
		builder.SetRevokedAt(*session.RevokedAt)
	} else {
		builder.ClearRevokedAt()
	}

	updated, err := builder.Save(ctx)
	if err != nil {
		return err
	}
	applyDesktopSessionEntity(session, updated)
	return nil
}

func (r *desktopSessionRepository) Revoke(ctx context.Context, sessionID string, userID int64, revokedAt time.Time) error {
	client := clientFromContext(ctx, r.client)
	record, err := client.DesktopSession.Query().
		Where(
			dbdesktopsession.SessionIDEQ(sessionID),
			dbdesktopsession.UserIDEQ(userID),
		).
		Only(ctx)
	if err != nil {
		if dbent.IsNotFound(err) {
			return service.ErrDesktopSessionNotFound
		}
		return err
	}
	_, err = record.Update().
		SetStatus("revoked").
		SetRevokedAt(revokedAt).
		Save(ctx)
	return err
}

func resolveDesktopSessionEntity(ctx context.Context, client *dbent.Client, id int64, sessionID string) (*dbent.DesktopSession, error) {
	if id > 0 {
		record, err := client.DesktopSession.Get(ctx, id)
		if err == nil {
			return record, nil
		}
		if !dbent.IsNotFound(err) || sessionID == "" {
			return nil, err
		}
	}

	return client.DesktopSession.Query().
		Where(dbdesktopsession.SessionIDEQ(sessionID)).
		Only(ctx)
}

func desktopSessionEntityToService(record *dbent.DesktopSession) *service.DesktopSession {
	if record == nil {
		return nil
	}
	result := &service.DesktopSession{}
	applyDesktopSessionEntity(result, record)
	return result
}

func applyDesktopSessionEntity(dst *service.DesktopSession, src *dbent.DesktopSession) {
	if dst == nil || src == nil {
		return
	}
	dst.ID = src.ID
	dst.SessionID = src.SessionID
	dst.UserID = src.UserID
	dst.GroupID = src.GroupID
	dst.DeviceID = src.DeviceID
	dst.DeviceName = src.DeviceName
	dst.Target = src.Target
	dst.Status = src.Status
	dst.RuntimeTokenHash = src.RuntimeTokenHash
	dst.ProfileKey = src.ProfileKey
	dst.ExpiresAt = src.ExpiresAt
	dst.LastSeenAt = src.LastSeenAt
	dst.RevokedAt = src.RevokedAt
	dst.CreatedAt = src.CreatedAt
	dst.UpdatedAt = src.UpdatedAt
}
