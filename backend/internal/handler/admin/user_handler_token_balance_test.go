package admin

import (
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/service"
	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/require"
)

type tokenWalletRepoStub struct {
	summary          *service.ClientBillingSummary
	balanceMilli     int64
	creditCalls      int
	debitCalls       int
	lastCreditAmount int64
	lastDebitAmount  int64
}

func (s *tokenWalletRepoStub) Credit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	s.creditCalls++
	s.lastCreditAmount = amountMilli
	s.balanceMilli += amountMilli
	if s.summary != nil {
		s.summary.RemainingMilliTokens += amountMilli
		s.summary.RechargedMilliTokens += amountMilli
	}
	return &service.ClientTokenWalletBalance{
		UserID:                    userID,
		ChannelID:                 channelID,
		BalanceMilliTokens:        s.balanceMilli,
		TotalRechargedMilliTokens: amountMilli,
	}, nil
}

func (s *tokenWalletRepoStub) Debit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	s.debitCalls++
	s.lastDebitAmount = amountMilli
	s.balanceMilli -= amountMilli
	if s.summary != nil {
		s.summary.RemainingMilliTokens -= amountMilli
		s.summary.ConsumedMilliTokens += amountMilli
	}
	return &service.ClientTokenWalletBalance{
		UserID:             userID,
		ChannelID:          channelID,
		BalanceMilliTokens: s.balanceMilli,
	}, nil
}

func (s *tokenWalletRepoStub) GetBalance(ctx context.Context, userID, channelID int64) (int64, error) {
	return s.balanceMilli, nil
}

func (s *tokenWalletRepoStub) GetSummary(ctx context.Context, userID int64) (*service.ClientBillingSummary, error) {
	if s.summary == nil {
		return &service.ClientBillingSummary{TokenUnit: "token"}, nil
	}
	return s.summary, nil
}

type tokenChannelResolverStub struct{}

func (s *tokenChannelResolverStub) GetChannelForGroup(ctx context.Context, groupID int64) (*service.Channel, error) {
	return &service.Channel{
		ID:             18,
		Status:         service.StatusActive,
		SettlementUnit: service.SettlementUnitToken,
	}, nil
}

func TestUserHandlerTokenBalanceEndpoints(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	adminSvc := newStubAdminService()
	walletRepo := &tokenWalletRepoStub{
		balanceMilli: 100_000_000_000,
		summary: &service.ClientBillingSummary{
			RemainingMilliTokens: 100_000_000_000,
			RechargedMilliTokens: 150_000_000_000,
			ConsumedMilliTokens:  50_000_000_000,
			TokenUnit:            "token",
		},
	}
	tokenSvc := service.NewClientTokenBillingService(walletRepo, &tokenChannelResolverStub{})
	userHandler := NewUserHandler(adminSvc, nil, tokenSvc)

	router.GET("/api/v1/admin/users/:id/token-balance", userHandler.GetTokenBalance)
	router.POST("/api/v1/admin/users/:id/token-balance", userHandler.UpdateTokenBalance)

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/v1/admin/users/1/token-balance", nil)
	router.ServeHTTP(rec, req)
	require.Equal(t, http.StatusOK, rec.Code)
	require.Contains(t, rec.Body.String(), "\"remaining_tokens\":100000000")

	rec = httptest.NewRecorder()
	req = httptest.NewRequest(http.MethodPost, "/api/v1/admin/users/1/token-balance", bytes.NewBufferString(`{"tokens":50000000,"operation":"add","group_id":2,"notes":"manual topup"}`))
	req.Header.Set("Content-Type", "application/json")
	router.ServeHTTP(rec, req)
	require.Equal(t, http.StatusOK, rec.Code)
	require.Equal(t, 1, walletRepo.creditCalls)
	require.Contains(t, rec.Body.String(), "\"recharged_tokens\":200000000")
}
