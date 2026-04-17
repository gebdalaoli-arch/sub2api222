package middleware

import (
	"context"
	"errors"
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

type desktopRuntimeAuthGroupReader interface {
	GetByID(ctx context.Context, id int64) (*service.Group, error)
}

type desktopRuntimeAuthSubscriptionReader interface {
	GetActiveSubscription(ctx context.Context, userID, groupID int64) (*service.UserSubscription, error)
}

type desktopRuntimeTokenValidator interface {
	ValidateRuntimeToken(ctx context.Context, token string) (*service.DesktopSession, error)
}

type desktopRuntimeAuthDependencies interface {
	desktopRuntimeTokenValidator
	GetUserByID(ctx context.Context, id int64) (*service.User, error)
	GetGroupByID(ctx context.Context, id int64) (*service.Group, error)
	GetActiveSubscription(ctx context.Context, userID, groupID int64) (*service.UserSubscription, error)
}

type desktopRuntimeAuthDeps struct {
	sessions      desktopRuntimeTokenValidator
	users         desktopRuntimeAuthUserReader
	groups        desktopRuntimeAuthGroupReader
	subscriptions desktopRuntimeAuthSubscriptionReader
}

func (d desktopRuntimeAuthDeps) ValidateRuntimeToken(ctx context.Context, token string) (*service.DesktopSession, error) {
	return d.sessions.ValidateRuntimeToken(ctx, token)
}

func (d desktopRuntimeAuthDeps) GetUserByID(ctx context.Context, id int64) (*service.User, error) {
	return d.users.GetByID(ctx, id)
}

func (d desktopRuntimeAuthDeps) GetGroupByID(ctx context.Context, id int64) (*service.Group, error) {
	if d.groups == nil {
		return nil, service.ErrGroupNotFound
	}
	return d.groups.GetByID(ctx, id)
}

func (d desktopRuntimeAuthDeps) GetActiveSubscription(ctx context.Context, userID, groupID int64) (*service.UserSubscription, error) {
	if d.subscriptions == nil {
		return nil, service.ErrSubscriptionNotFound
	}
	return d.subscriptions.GetActiveSubscription(ctx, userID, groupID)
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
		if err != nil || session == nil || session.GroupID <= 0 {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		user, err := deps.GetUserByID(c.Request.Context(), session.UserID)
		if err != nil || user == nil {
			AbortWithError(c, 401, "USER_NOT_FOUND", "User not found")
			return
		}
		if !user.IsActive() {
			AbortWithError(c, 401, "USER_INACTIVE", "User account is not active")
			return
		}

		group, err := deps.GetGroupByID(c.Request.Context(), session.GroupID)
		if err != nil || !service.IsGroupContextValid(group) {
			AbortWithError(c, 401, "INVALID_RUNTIME_TOKEN", "Invalid runtime token")
			return
		}

		apiKey := buildDesktopRuntimeAPIKey(session, user, group)

		if group.IsSubscriptionType() {
			subscription, subErr := deps.GetActiveSubscription(c.Request.Context(), user.ID, group.ID)
			if subErr != nil {
				if errors.Is(subErr, service.ErrSubscriptionNotFound) {
					AbortWithError(c, 403, "SUBSCRIPTION_NOT_FOUND", "No active subscription found for this group")
					return
				}
				AbortWithError(c, 500, "INTERNAL_ERROR", "Failed to load subscription")
				return
			}
			if subscription == nil {
				AbortWithError(c, 403, "SUBSCRIPTION_NOT_FOUND", "No active subscription found for this group")
				return
			}
			c.Set(string(ContextKeySubscription), subscription)
		}

		c.Set(contextKeyDesktopSession, session)
		c.Set(string(ContextKeyAPIKey), apiKey)
		c.Set(string(ContextKeyUser), AuthSubject{UserID: user.ID, Concurrency: user.Concurrency})
		c.Set(string(ContextKeyUserRole), user.Role)
		setGroupContext(c, group)
		c.Next()
	})
}

func ProvideDesktopRuntimeAuthMiddleware(
	sessionService *service.DesktopSessionService,
	userService *service.UserService,
	groupService *service.GroupService,
	subscriptionService *service.SubscriptionService,
) DesktopRuntimeAuthMiddleware {
	return NewDesktopRuntimeAuthMiddleware(desktopRuntimeAuthDeps{
		sessions:      sessionService,
		users:         userService,
		groups:        groupService,
		subscriptions: subscriptionService,
	})
}

func buildDesktopRuntimeAPIKey(session *service.DesktopSession, user *service.User, group *service.Group) *service.APIKey {
	apiKey := &service.APIKey{
		UserID: user.ID,
		Key:    desktopRuntimeSyntheticAPIKeyName + ":" + session.SessionID,
		Name:   desktopRuntimeSyntheticAPIKeyName,
		Status: service.StatusActive,
		User:   user,
		Group:  group,
	}
	if group != nil {
		groupID := group.ID
		apiKey.GroupID = &groupID
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
