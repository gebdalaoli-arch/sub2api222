package middleware

import (
	"context"
	"strings"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
)

const (
	contextKeyDesktopSession          = "desktop_session"
	desktopRuntimeSyntheticAPIKeyName = "desktop-runtime"
)

type desktopRuntimeAuthUserReader interface {
	GetByID(ctx context.Context, id int64) (*service.User, error)
}

type desktopRuntimeTokenValidator interface {
	ValidateRuntimeToken(ctx context.Context, token string) (*service.DesktopSession, error)
}

type desktopRuntimeAuthDependencies interface {
	desktopRuntimeTokenValidator
	desktopRuntimeAuthUserReader
}

type desktopRuntimeAuthDeps struct {
	sessions desktopRuntimeTokenValidator
	users    desktopRuntimeAuthUserReader
}

func (d desktopRuntimeAuthDeps) ValidateRuntimeToken(ctx context.Context, token string) (*service.DesktopSession, error) {
	return d.sessions.ValidateRuntimeToken(ctx, token)
}

func (d desktopRuntimeAuthDeps) GetByID(ctx context.Context, id int64) (*service.User, error) {
	return d.users.GetByID(ctx, id)
}

type DesktopRuntimeAuthMiddleware gin.HandlerFunc

func NewDesktopRuntimeAuthMiddleware(deps desktopRuntimeAuthDependencies) DesktopRuntimeAuthMiddleware {
	return DesktopRuntimeAuthMiddleware(func(c *gin.Context) {
		authHeader := strings.TrimSpace(c.GetHeader("Authorization"))
		if authHeader == "" {
			AbortWithError(c, 401, "RUNTIME_TOKEN_REQUIRED", "Runtime token is required")
			return
		}

		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "Bearer") {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		token := strings.TrimSpace(parts[1])
		if token == "" {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		session, err := deps.ValidateRuntimeToken(c.Request.Context(), token)
		if err != nil {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		user, err := deps.GetByID(c.Request.Context(), session.UserID)
		if err != nil || user == nil {
			AbortWithError(c, 401, "USER_NOT_FOUND", "User not found")
			return
		}
		if !user.IsActive() {
			AbortWithError(c, 401, "USER_INACTIVE", "User account is not active")
			return
		}

		apiKey := buildDesktopRuntimeAPIKey(session, user)

		c.Set(contextKeyDesktopSession, session)
		c.Set(string(ContextKeyAPIKey), apiKey)
		c.Set(string(ContextKeyUser), AuthSubject{UserID: user.ID, Concurrency: user.Concurrency})
		c.Set(string(ContextKeyUserRole), user.Role)
		c.Next()
	})
}

func ProvideDesktopRuntimeAuthMiddleware(sessionService *service.DesktopSessionService, userService *service.UserService) DesktopRuntimeAuthMiddleware {
	return NewDesktopRuntimeAuthMiddleware(desktopRuntimeAuthDeps{
		sessions: sessionService,
		users:    userService,
	})
}

func buildDesktopRuntimeAPIKey(session *service.DesktopSession, user *service.User) *service.APIKey {
	apiKey := &service.APIKey{
		UserID: user.ID,
		Key:    desktopRuntimeSyntheticAPIKeyName + ":" + session.SessionID,
		Name:   desktopRuntimeSyntheticAPIKeyName,
		Status: service.StatusActive,
		User:   user,
	}
	if session != nil {
		apiKey.CreatedAt = session.CreatedAt
		apiKey.UpdatedAt = session.UpdatedAt
		if session.ProfileKey != "" {
			apiKey.Name = session.ProfileKey
		}
	}
	return apiKey
}
