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

type desktopRuntimeAuthAPIKeyStore interface {
	Create(ctx context.Context, key *service.APIKey) error
	GetByKey(ctx context.Context, key string) (*service.APIKey, error)
}

type desktopRuntimeAuthAPIKeyResolver interface {
	ResolveRuntimeAPIKey(ctx context.Context, session *service.DesktopSession, user *service.User, group *service.Group) (*service.APIKey, error)
}

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
	apiKeys       desktopRuntimeAuthAPIKeyStore
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

func (d desktopRuntimeAuthDeps) ResolveRuntimeAPIKey(ctx context.Context, session *service.DesktopSession, user *service.User, group *service.Group) (*service.APIKey, error) {
	return resolveDesktopRuntimeAPIKey(ctx, d.apiKeys, session, user, group)
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
		if resolver, ok := deps.(desktopRuntimeAuthAPIKeyResolver); ok {
			apiKey, err = resolver.ResolveRuntimeAPIKey(c.Request.Context(), session, user, group)
			if err != nil || apiKey == nil {
				AbortWithError(c, 500, "INTERNAL_ERROR", "Failed to load runtime API key")
				return
			}
			if apiKey.ID <= 0 {
				AbortWithError(c, 500, "INTERNAL_ERROR", "Runtime API key is not ready")
				return
			}
		}

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
	apiKeys desktopRuntimeAuthAPIKeyStore,
) DesktopRuntimeAuthMiddleware {
	return NewDesktopRuntimeAuthMiddleware(desktopRuntimeAuthDeps{
		sessions:      sessionService,
		users:         userService,
		groups:        groupService,
		subscriptions: subscriptionService,
		apiKeys:       apiKeys,
	})
}

func buildDesktopRuntimeAPIKey(session *service.DesktopSession, user *service.User, group *service.Group) *service.APIKey {
	profileKey := desktopRuntimeSyntheticAPIKeyName
	groupID := int64(0)
	if group != nil {
		groupID = group.ID
	}
	if session != nil && session.ProfileKey != "" {
		profileKey = session.ProfileKey
	}

	apiKey := &service.APIKey{
		UserID: user.ID,
		Key:    service.BuildDesktopRuntimeSyntheticAPIKey(user.ID, groupID, profileKey),
		Name:   profileKey,
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
	}
	return apiKey
}

func resolveDesktopRuntimeAPIKey(
	ctx context.Context,
	store desktopRuntimeAuthAPIKeyStore,
	session *service.DesktopSession,
	user *service.User,
	group *service.Group,
) (*service.APIKey, error) {
	fallback := buildDesktopRuntimeAPIKey(session, user, group)
	if store == nil {
		return fallback, nil
	}

	existing, err := store.GetByKey(ctx, fallback.Key)
	if err == nil {
		return hydrateDesktopRuntimeAPIKey(existing, fallback, user, group), nil
	}
	if !errors.Is(err, service.ErrAPIKeyNotFound) {
		return nil, err
	}

	if err := store.Create(ctx, fallback); err != nil {
		if !errors.Is(err, service.ErrAPIKeyExists) {
			return nil, err
		}
		existing, getErr := store.GetByKey(ctx, fallback.Key)
		if getErr != nil {
			return nil, getErr
		}
		return hydrateDesktopRuntimeAPIKey(existing, fallback, user, group), nil
	}

	return hydrateDesktopRuntimeAPIKey(fallback, fallback, user, group), nil
}

func hydrateDesktopRuntimeAPIKey(
	apiKey *service.APIKey,
	fallback *service.APIKey,
	user *service.User,
	group *service.Group,
) *service.APIKey {
	if apiKey == nil {
		return fallback
	}
	if apiKey.User == nil {
		apiKey.User = user
	}
	if apiKey.Group == nil {
		apiKey.Group = group
	}
	if apiKey.UserID == 0 && user != nil {
		apiKey.UserID = user.ID
	}
	if apiKey.GroupID == nil && group != nil {
		groupID := group.ID
		apiKey.GroupID = &groupID
	}
	if strings.TrimSpace(apiKey.Name) == "" && fallback != nil {
		apiKey.Name = fallback.Name
	}
	if strings.TrimSpace(apiKey.Key) == "" && fallback != nil {
		apiKey.Key = fallback.Key
	}
	if strings.TrimSpace(apiKey.Status) == "" {
		apiKey.Status = service.StatusActive
	}
	return apiKey
}
