package middleware

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type desktopRuntimeAuthServiceStub struct {
	lastToken string
	session   *service.DesktopSession
	user      *service.User
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

func (s *desktopRuntimeAuthServiceStub) GetByID(_ context.Context, id int64) (*service.User, error) {
	if s.user != nil {
		return s.user, nil
	}
	return &service.User{ID: id, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 3}, nil
}

func TestDesktopRuntimeAuthMiddleware_AcceptsRuntimeToken(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopRuntimeAuthServiceStub{
		session: &service.DesktopSession{
			SessionID: "sess-1",
			UserID:    42,
			Target:    string(service.DesktopSessionTargetDesktop),
			Status:    service.StatusActive,
		},
		user: &service.User{ID: 42, Role: service.RoleUser, Status: service.StatusActive, Concurrency: 5},
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
		require.Equal(t, int64(42), apiKey.UserID)
		require.Equal(t, service.StatusActive, apiKey.Status)
		require.NotNil(t, apiKey.User)
		require.Equal(t, int64(42), apiKey.User.ID)
		require.Equal(t, service.RoleUser, apiKey.User.Role)
		require.Equal(t, service.StatusActive, apiKey.User.Status)

		c.Status(http.StatusNoContent)
	})

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/v1/responses", nil)
	req.Header.Set("Authorization", "Bearer runtime-token-1")
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
	require.Equal(t, "runtime-token-1", svc.lastToken)
}
