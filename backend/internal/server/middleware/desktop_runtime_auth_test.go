package middleware

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/Wei-Shaw/sub2api/internal/pkg/ctxkey"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type desktopRuntimeAuthServiceStub struct {
	lastToken string
	session   *service.DesktopSession
	user      *service.User
	group     *service.Group
	sub       *service.UserSubscription
	apiKey    *service.APIKey
	err       error
}

func (s *desktopRuntimeAuthServiceStub) ValidateRuntimeToken(_ context.Context, token string) (*service.DesktopSession, error) {
	s.lastToken = token
	if s.err != nil {
		return nil, s.err
	}
	if s.session != nil {
		return s.session, nil
	}
	return &service.DesktopSession{UserID: 7}, nil
}

func (s *desktopRuntimeAuthServiceStub) GetUserByID(_ context.Context, id int64) (*service.User, error) {
	if s.user != nil {
		return s.user, nil
	}
	return &service.User{ID: id, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 3}, nil
}

func (s *desktopRuntimeAuthServiceStub) GetGroupByID(_ context.Context, id int64) (*service.Group, error) {
	if s.group != nil {
		clone := *s.group
		clone.ID = id
		return &clone, nil
	}
	return &service.Group{ID: id, Platform: service.PlatformOpenAI, Status: service.StatusActive, Hydrated: true}, nil
}

func (s *desktopRuntimeAuthServiceStub) GetActiveSubscription(_ context.Context, userID, groupID int64) (*service.UserSubscription, error) {
	if s.sub != nil {
		clone := *s.sub
		clone.UserID = userID
		clone.GroupID = groupID
		return &clone, nil
	}
	return nil, service.ErrSubscriptionNotFound
}

func (s *desktopRuntimeAuthServiceStub) ResolveRuntimeAPIKey(_ context.Context, session *service.DesktopSession, user *service.User, group *service.Group) (*service.APIKey, error) {
	if s.apiKey == nil {
		return buildDesktopRuntimeAPIKey(session, user, group), nil
	}
	clone := *s.apiKey
	if clone.User == nil {
		clone.User = user
	}
	if clone.Group == nil {
		clone.Group = group
	}
	if clone.UserID == 0 && user != nil {
		clone.UserID = user.ID
	}
	if clone.GroupID == nil && group != nil {
		groupID := group.ID
		clone.GroupID = &groupID
	}
	if clone.Status == "" {
		clone.Status = service.StatusActive
	}
	if clone.Key == "" || clone.Name == "" {
		fallback := buildDesktopRuntimeAPIKey(session, user, group)
		if clone.Key == "" {
			clone.Key = fallback.Key
		}
		if clone.Name == "" {
			clone.Name = fallback.Name
		}
	}
	return &clone, nil
}

func TestDesktopRuntimeAuthMiddleware_AcceptsRuntimeToken(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopRuntimeAuthServiceStub{
		session: &service.DesktopSession{
			SessionID: "sess-1",
			UserID:    42,
			GroupID:   9,
			Target:    string(service.DesktopSessionTargetDesktop),
			Status:    service.StatusActive,
		},
		user: &service.User{ID: 42, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 5},
		group: &service.Group{
			ID:               9,
			Platform:         service.PlatformOpenAI,
			Status:           service.StatusActive,
			Hydrated:         true,
			SubscriptionType: service.SubscriptionTypeSubscription,
		},
		sub: &service.UserSubscription{
			ID:        77,
			UserID:    42,
			GroupID:   9,
			Status:    service.SubscriptionStatusActive,
			ExpiresAt: time.Date(2026, 4, 19, 9, 0, 0, 0, time.UTC),
		},
		apiKey: &service.APIKey{
			ID:     701,
			UserID: 42,
			Status: service.StatusActive,
		},
	}
	mw := NewDesktopRuntimeAuthMiddleware(svc)

	r := gin.New()
	r.GET("/api/desktop/v1/responses", gin.HandlerFunc(mw), func(c *gin.Context) {
		subject, ok := GetAuthSubjectFromContext(c)
		require.True(t, ok)
		require.Equal(t, int64(42), subject.UserID)
		require.Equal(t, 5, subject.Concurrency)

		apiKey, ok := GetAPIKeyFromContext(c)
		require.True(t, ok)
		require.NotZero(t, apiKey.ID)
		require.Equal(t, int64(42), apiKey.UserID)
		require.Equal(t, service.StatusActive, apiKey.Status)
		require.NotNil(t, apiKey.GroupID)
		require.Equal(t, int64(9), *apiKey.GroupID)
		require.NotNil(t, apiKey.Group)
		require.Equal(t, int64(9), apiKey.Group.ID)
		require.NotNil(t, apiKey.User)
		require.Equal(t, int64(42), apiKey.User.ID)
		require.Equal(t, service.RoleUser, apiKey.User.Role)
		require.Equal(t, service.StatusActive, apiKey.User.Status)

		subscription, ok := GetSubscriptionFromContext(c)
		require.True(t, ok)
		require.Equal(t, int64(77), subscription.ID)
		groupFromCtx, ok := c.Request.Context().Value(ctxkey.Group).(*service.Group)
		require.True(t, ok)
		require.Equal(t, int64(9), groupFromCtx.ID)

		c.Status(http.StatusNoContent)
	})

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/v1/responses", nil)
	req.Header.Set("Authorization", "Bearer runtime-token-1")
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
	require.Equal(t, "runtime-token-1", svc.lastToken)
}
