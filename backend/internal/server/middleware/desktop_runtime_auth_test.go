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

func TestDesktopRuntimeAuthMiddleware_AcceptsRuntimeToken(t *testing.T) {
	gin.SetMode(gin.TestMode)

	svc := &desktopRuntimeAuthServiceStub{
		session: &service.DesktopSession{UserID: 42},
	}
	mw := NewDesktopRuntimeAuthMiddleware(svc)

	r := gin.New()
	r.GET("/api/desktop/v1/responses", gin.HandlerFunc(mw), func(c *gin.Context) {
		subject, ok := GetAuthSubjectFromContext(c)
		require.True(t, ok)
		require.Equal(t, int64(42), subject.UserID)
		c.Status(http.StatusNoContent)
	})

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/v1/responses", nil)
	req.Header.Set("Authorization", "Bearer runtime-token-1")
	resp := httptest.NewRecorder()

	r.ServeHTTP(resp, req)

	require.Equal(t, http.StatusNoContent, resp.Code)
	require.Equal(t, "runtime-token-1", svc.lastToken)
}
