package routes

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/Wei-Shaw/sub2api/internal/handler"
	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
	servermiddleware "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type desktopRoutesRuntimeAuthStub struct {
	session *service.DesktopSession
	user    *service.User
	group   *service.Group
	sub     *service.UserSubscription
}

func (s *desktopRoutesRuntimeAuthStub) ValidateRuntimeToken(_ context.Context, _ string) (*service.DesktopSession, error) {
	if s.session != nil {
		return s.session, nil
	}
	return &service.DesktopSession{
		SessionID: "sess-route-1",
		UserID:    9,
		GroupID:   9,
		Target:    string(service.DesktopSessionTargetDesktop),
		Status:    service.StatusActive,
	}, nil
}

func (s *desktopRoutesRuntimeAuthStub) GetUserByID(_ context.Context, id int64) (*service.User, error) {
	if s.user != nil {
		return s.user, nil
	}
	return &service.User{ID: id, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 2}, nil
}

func (s *desktopRoutesRuntimeAuthStub) GetGroupByID(_ context.Context, id int64) (*service.Group, error) {
	if s.group != nil {
		clone := *s.group
		clone.ID = id
		return &clone, nil
	}
	return &service.Group{
		ID:               id,
		Platform:         service.PlatformOpenAI,
		Status:           service.StatusActive,
		Hydrated:         true,
		SubscriptionType: service.SubscriptionTypeStandard,
	}, nil
}

func (s *desktopRoutesRuntimeAuthStub) GetActiveSubscription(_ context.Context, userID, groupID int64) (*service.UserSubscription, error) {
	if s.sub != nil {
		clone := *s.sub
		clone.UserID = userID
		clone.GroupID = groupID
		return &clone, nil
	}
	return nil, service.ErrSubscriptionNotFound
}

type desktopRoutesSessionServiceStub struct {
	createReq        service.DesktopSessionCreateRequest
	refreshSessionID string
	refreshUserID    int64
	revokeSessionID  string
	revokeUserID     int64
	refreshErr       error
	revokeErr        error
}

func (s *desktopRoutesSessionServiceStub) Create(_ context.Context, req service.DesktopSessionCreateRequest) (*service.DesktopSessionResult, error) {
	s.createReq = req
	return &service.DesktopSessionResult{
		SessionID:      "sess-route-created",
		UserID:         req.UserID,
		RuntimeToken:   "runtime-route-token",
		ProfileKey:     "platform-" + string(req.Target),
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC),
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *desktopRoutesSessionServiceStub) Refresh(_ context.Context, sessionID string, userID int64) (*service.DesktopSessionResult, error) {
	s.refreshSessionID = sessionID
	s.refreshUserID = userID
	if s.refreshErr != nil {
		return nil, s.refreshErr
	}
	return &service.DesktopSessionResult{
		SessionID:      sessionID,
		UserID:         userID,
		ProfileKey:     "platform-desktop",
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC),
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *desktopRoutesSessionServiceStub) Revoke(_ context.Context, sessionID string, userID int64) error {
	s.revokeSessionID = sessionID
	s.revokeUserID = userID
	return s.revokeErr
}

func newDesktopRoutesSessionRouter(svc *desktopRoutesSessionServiceStub) *gin.Engine {
	router := gin.New()
	v1 := router.Group("/api/v1")

	RegisterDesktopRoutes(
		router,
		v1,
		&handler.Handlers{
			Desktop:       handler.NewDesktopHandler(svc),
			OpenAIGateway: &handler.OpenAIGatewayHandler{},
		},
		servermiddleware.JWTAuthMiddleware(func(c *gin.Context) {
			c.Set(string(servermiddleware.ContextKeyUser), servermiddleware.AuthSubject{UserID: 42})
			c.Next()
		}),
		servermiddleware.DesktopRuntimeAuthMiddleware(func(c *gin.Context) { c.Next() }),
		nil,
	)

	return router
}

func TestDesktopRoutesRuntimeResponsesRouteDoesNotFailWithMissingAPIKeyContext(t *testing.T) {
	gin.SetMode(gin.TestMode)

	router := gin.New()
	v1 := router.Group("/api/v1")

	RegisterDesktopRoutes(
		router,
		v1,
		&handler.Handlers{
			Desktop:       &handler.DesktopHandler{},
			OpenAIGateway: &handler.OpenAIGatewayHandler{},
		},
		servermiddleware.JWTAuthMiddleware(func(c *gin.Context) { c.Next() }),
		servermiddleware.NewDesktopRuntimeAuthMiddleware(&desktopRoutesRuntimeAuthStub{
			session: &service.DesktopSession{
				SessionID: "sess-route-1",
				UserID:    7,
				GroupID:   9,
				Target:    string(service.DesktopSessionTargetDesktop),
				Status:    service.StatusActive,
			},
			user: &service.User{ID: 7, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 4},
		}),
		nil,
	)

	req := httptest.NewRequest(http.MethodPost, "/api/desktop/v1/responses", strings.NewReader(`{"model":"gpt-5"}`))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer runtime-route-token")
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusServiceUnavailable, resp.Code)
	require.NotContains(t, resp.Body.String(), "Invalid API key")
}

func TestDesktopRoutesRuntimeResponsesCompactRouteDoesNotFailWithMissingAPIKeyContext(t *testing.T) {
	gin.SetMode(gin.TestMode)

	router := gin.New()
	v1 := router.Group("/api/v1")

	RegisterDesktopRoutes(
		router,
		v1,
		&handler.Handlers{
			Desktop:       &handler.DesktopHandler{},
			OpenAIGateway: &handler.OpenAIGatewayHandler{},
		},
		servermiddleware.JWTAuthMiddleware(func(c *gin.Context) { c.Next() }),
		servermiddleware.NewDesktopRuntimeAuthMiddleware(&desktopRoutesRuntimeAuthStub{
			session: &service.DesktopSession{
				SessionID: "sess-route-1",
				UserID:    7,
				GroupID:   9,
				Target:    string(service.DesktopSessionTargetDesktop),
				Status:    service.StatusActive,
			},
			user: &service.User{ID: 7, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 4},
		}),
		nil,
	)

	req := httptest.NewRequest(http.MethodPost, "/api/desktop/v1/responses/compact", strings.NewReader(`{"model":"gpt-5"}`))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer runtime-route-token")
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusServiceUnavailable, resp.Code)
	require.NotContains(t, resp.Body.String(), "Invalid API key")
}

func TestDesktopRoutesCreateSessionRejectsMissingGroupID(t *testing.T) {
	gin.SetMode(gin.TestMode)

	router := newDesktopRoutesSessionRouter(&desktopRoutesSessionServiceStub{})
	req := httptest.NewRequest(http.MethodPost, "/api/v1/desktop/sessions", strings.NewReader(`{"target":"desktop","device_id":"desktop-1","device_name":"mbp","client_version":"0.1.0"}`))
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusBadRequest, resp.Code)
	require.Contains(t, resp.Body.String(), "GroupID")
}

func TestDesktopRoutesRefreshSessionRoutePropagatesNotFound(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopRoutesSessionServiceStub{
		refreshErr: infraerrors.NotFound("DESKTOP_SESSION_NOT_FOUND", "desktop session not found"),
	}
	router := newDesktopRoutesSessionRouter(svc)
	req := httptest.NewRequest(http.MethodPost, "/api/v1/desktop/sessions/sess-route-1/refresh", nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNotFound, resp.Code)
	require.Equal(t, "sess-route-1", svc.refreshSessionID)
	require.Equal(t, int64(42), svc.refreshUserID)
}

func TestDesktopRoutesDeleteSessionRoutePropagatesNotFound(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopRoutesSessionServiceStub{
		revokeErr: infraerrors.NotFound("DESKTOP_SESSION_NOT_FOUND", "desktop session not found"),
	}
	router := newDesktopRoutesSessionRouter(svc)
	req := httptest.NewRequest(http.MethodDelete, "/api/v1/desktop/sessions/sess-route-1", nil)
	resp := httptest.NewRecorder()

	router.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNotFound, resp.Code)
	require.Equal(t, "sess-route-1", svc.revokeSessionID)
	require.Equal(t, int64(42), svc.revokeUserID)
}
