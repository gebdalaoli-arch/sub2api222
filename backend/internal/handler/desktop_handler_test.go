package handler

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
	middleware2 "github.com/Wei-Shaw/sub2api/internal/server/middleware"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type desktopHandlerServiceStub struct {
	lastCreate  service.DesktopSessionCreateRequest
	lastRefresh struct {
		sessionID string
		userID    int64
	}
	lastRevoke struct {
		sessionID string
		userID    int64
	}
	createResp *service.DesktopSessionResult
	createErr  error
	refreshErr error
	revokeErr  error
}

func (s *desktopHandlerServiceStub) Create(_ context.Context, req service.DesktopSessionCreateRequest) (*service.DesktopSessionResult, error) {
	s.lastCreate = req
	if s.createErr != nil {
		return nil, s.createErr
	}
	if s.createResp != nil {
		return s.createResp, nil
	}
	return &service.DesktopSessionResult{
		SessionID:      "sess_123",
		UserID:         req.UserID,
		RuntimeToken:   "runtime-token-1",
		ProfileKey:     "platform-desktop",
		RefreshAfter:   30 * time.Minute,
		ExpiresAt:      time.Date(2026, 1, 2, 3, 4, 5, 0, time.UTC),
		GatewayBaseURL: "/api/desktop/v1",
	}, nil
}

func (s *desktopHandlerServiceStub) Refresh(_ context.Context, sessionID string, userID int64) (*service.DesktopSessionResult, error) {
	s.lastRefresh.sessionID = sessionID
	s.lastRefresh.userID = userID
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

func (s *desktopHandlerServiceStub) Revoke(_ context.Context, sessionID string, userID int64) error {
	s.lastRevoke.sessionID = sessionID
	s.lastRevoke.userID = userID
	return s.revokeErr
}

func TestDesktopHandler_CreateSession(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopHandlerServiceStub{}
	h := NewDesktopHandler(svc)

	r := gin.New()
	r.POST("/api/v1/desktop/sessions", func(c *gin.Context) {
		c.Set(string(middleware2.ContextKeyUser), middleware2.AuthSubject{UserID: 42})
		h.CreateSession(c)
	})

	req := httptest.NewRequest(
		http.MethodPost,
		"/api/v1/desktop/sessions",
		strings.NewReader(`{"target":"desktop","group_id":9,"device_id":"d-1","device_name":"mbp","client_version":"0.1.0"}`),
	)
	req.Header.Set("Content-Type", "application/json")
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusOK, resp.Code)
	require.Contains(t, resp.Body.String(), "\"session_id\"")
	require.Equal(t, int64(42), svc.lastCreate.UserID)
	require.Equal(t, int64(9), svc.lastCreate.GroupID)
	require.Equal(t, service.DesktopSessionTargetDesktop, svc.lastCreate.Target)
	require.Equal(t, "d-1", svc.lastCreate.DeviceID)
	require.Equal(t, "mbp", svc.lastCreate.DeviceName)
	require.Equal(t, "0.1.0", svc.lastCreate.ClientVersion)
}

func TestDesktopHandler_RefreshSessionPassesOwnerAndPropagatesNotFound(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopHandlerServiceStub{refreshErr: infraerrors.NotFound("DESKTOP_SESSION_NOT_FOUND", "desktop session not found")}
	h := NewDesktopHandler(svc)

	r := gin.New()
	r.POST("/api/v1/desktop/sessions/:id/refresh", func(c *gin.Context) {
		c.Set(string(middleware2.ContextKeyUser), middleware2.AuthSubject{UserID: 42})
		h.RefreshSession(c)
	})

	req := httptest.NewRequest(http.MethodPost, "/api/v1/desktop/sessions/sess-1/refresh", nil)
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNotFound, resp.Code)
	require.Equal(t, "sess-1", svc.lastRefresh.sessionID)
	require.Equal(t, int64(42), svc.lastRefresh.userID)
}

func TestDesktopHandler_DeleteSessionPassesOwnerAndPropagatesNotFound(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopHandlerServiceStub{revokeErr: infraerrors.NotFound("DESKTOP_SESSION_NOT_FOUND", "desktop session not found")}
	h := NewDesktopHandler(svc)

	r := gin.New()
	r.DELETE("/api/v1/desktop/sessions/:id", func(c *gin.Context) {
		c.Set(string(middleware2.ContextKeyUser), middleware2.AuthSubject{UserID: 42})
		h.DeleteSession(c)
	})

	req := httptest.NewRequest(http.MethodDelete, "/api/v1/desktop/sessions/sess-1", nil)
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNotFound, resp.Code)
	require.Equal(t, "sess-1", svc.lastRevoke.sessionID)
	require.Equal(t, int64(42), svc.lastRevoke.userID)
}
