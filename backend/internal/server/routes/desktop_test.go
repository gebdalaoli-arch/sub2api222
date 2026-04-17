package routes

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/handler"
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
