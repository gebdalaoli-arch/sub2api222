//go:build unit

package middleware

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/config"
	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type middlewareTokenWalletRepoStub struct {
	balanceMilli int64
}

func (s *middlewareTokenWalletRepoStub) Credit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	panic("unexpected Credit call")
}

func (s *middlewareTokenWalletRepoStub) Debit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	panic("unexpected Debit call")
}

func (s *middlewareTokenWalletRepoStub) GetBalance(ctx context.Context, userID, channelID int64) (int64, error) {
	return s.balanceMilli, nil
}

func (s *middlewareTokenWalletRepoStub) GetSummary(ctx context.Context, userID int64) (*service.ClientBillingSummary, error) {
	return &service.ClientBillingSummary{TokenUnit: "token"}, nil
}

type middlewareTokenChannelResolverStub struct {
	channel *service.Channel
}

func (s *middlewareTokenChannelResolverStub) GetChannelForGroup(ctx context.Context, groupID int64) (*service.Channel, error) {
	if s.channel == nil {
		return nil, nil
	}
	return s.channel.Clone(), nil
}

func newTokenAuthTestRouter(apiKeyService *service.APIKeyService, tokenBillingService *service.ClientTokenBillingService) *gin.Engine {
	router := gin.New()
	router.Use(gin.HandlerFunc(NewAPIKeyAuthMiddleware(apiKeyService, nil, tokenBillingService, &config.Config{})))
	router.GET("/v1/test", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"ok": true})
	})
	return router
}

func TestAPIKeyAuthAllowsTokenChannelWhenWalletHasBalance(t *testing.T) {
	groupID := int64(10)
	group := &service.Group{
		ID:               groupID,
		Platform:         service.PlatformOpenAI,
		Status:           service.StatusActive,
		SubscriptionType: service.SubscriptionTypeStandard,
	}
	user := &service.User{
		ID:      1,
		Role:    service.RoleUser,
		Status:  service.StatusActive,
		Balance: 0,
	}
	repo := &stubApiKeyRepo{
		getByKey: func(ctx context.Context, key string) (*service.APIKey, error) {
			return &service.APIKey{
				ID:      1,
				Key:     key,
				Status:  service.StatusAPIKeyActive,
				User:    user,
				Group:   group,
				GroupID: &groupID,
			}, nil
		},
	}

	apiKeyService := service.NewAPIKeyService(repo, nil, nil, nil, nil, nil, &config.Config{})
	tokenBillingService := service.NewClientTokenBillingService(
		&middlewareTokenWalletRepoStub{balanceMilli: 1000},
		&middlewareTokenChannelResolverStub{
			channel: &service.Channel{ID: 77, Status: service.StatusActive, SettlementUnit: service.SettlementUnitToken},
		},
	)
	router := newTokenAuthTestRouter(apiKeyService, tokenBillingService)

	req := httptest.NewRequest(http.MethodGet, "/v1/test", nil)
	req.Header.Set("Authorization", "Bearer sk-token")
	rec := httptest.NewRecorder()

	router.ServeHTTP(rec, req)

	require.Equal(t, http.StatusOK, rec.Code)
}

func TestAPIKeyAuthRejectsTokenChannelWhenWalletEmpty(t *testing.T) {
	groupID := int64(10)
	group := &service.Group{
		ID:               groupID,
		Platform:         service.PlatformOpenAI,
		Status:           service.StatusActive,
		SubscriptionType: service.SubscriptionTypeStandard,
	}
	user := &service.User{
		ID:      1,
		Role:    service.RoleUser,
		Status:  service.StatusActive,
		Balance: 0,
	}
	repo := &stubApiKeyRepo{
		getByKey: func(ctx context.Context, key string) (*service.APIKey, error) {
			return &service.APIKey{
				ID:      1,
				Key:     key,
				Status:  service.StatusAPIKeyActive,
				User:    user,
				Group:   group,
				GroupID: &groupID,
			}, nil
		},
	}

	apiKeyService := service.NewAPIKeyService(repo, nil, nil, nil, nil, nil, &config.Config{})
	tokenBillingService := service.NewClientTokenBillingService(
		&middlewareTokenWalletRepoStub{balanceMilli: 0},
		&middlewareTokenChannelResolverStub{
			channel: &service.Channel{ID: 77, Status: service.StatusActive, SettlementUnit: service.SettlementUnitToken},
		},
	)
	router := newTokenAuthTestRouter(apiKeyService, tokenBillingService)

	req := httptest.NewRequest(http.MethodGet, "/v1/test", nil)
	req.Header.Set("Authorization", "Bearer sk-token")
	rec := httptest.NewRecorder()

	router.ServeHTTP(rec, req)

	require.Equal(t, http.StatusForbidden, rec.Code)
	require.Contains(t, rec.Body.String(), "Insufficient token balance")
}
