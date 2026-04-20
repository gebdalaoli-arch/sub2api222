package service

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
)

type stubClientTokenWalletRepo struct {
	summary           *ClientBillingSummary
	balanceMilli      int64
	creditResult      *ClientTokenWalletBalance
	creditCalls       int
	creditUserID      int64
	creditChannelID   int64
	creditAmountMilli int64
	creditSourceType  string
	creditSourceID    string
	debitResult       *ClientTokenWalletBalance
	debitCalls        int
	debitUserID       int64
	debitChannelID    int64
	debitAmountMilli  int64
	debitSourceType   string
	debitSourceID     string
}

func (s *stubClientTokenWalletRepo) Credit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*ClientTokenWalletBalance, error) {
	s.creditCalls++
	s.creditUserID = userID
	s.creditChannelID = channelID
	s.creditAmountMilli = amountMilli
	s.creditSourceType = sourceType
	s.creditSourceID = sourceID
	if s.creditResult != nil {
		return s.creditResult, nil
	}
	return &ClientTokenWalletBalance{
		UserID:                    userID,
		ChannelID:                 channelID,
		BalanceMilliTokens:        amountMilli,
		TotalRechargedMilliTokens: amountMilli,
	}, nil
}

func (s *stubClientTokenWalletRepo) Debit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*ClientTokenWalletBalance, error) {
	s.debitCalls++
	s.debitUserID = userID
	s.debitChannelID = channelID
	s.debitAmountMilli = amountMilli
	s.debitSourceType = sourceType
	s.debitSourceID = sourceID
	if s.debitResult != nil {
		return s.debitResult, nil
	}
	return &ClientTokenWalletBalance{
		UserID:             userID,
		ChannelID:          channelID,
		BalanceMilliTokens: s.balanceMilli - amountMilli,
	}, nil
}

func (s *stubClientTokenWalletRepo) GetBalance(ctx context.Context, userID, channelID int64) (int64, error) {
	return s.balanceMilli, nil
}

func (s *stubClientTokenWalletRepo) GetSummary(ctx context.Context, userID int64) (*ClientBillingSummary, error) {
	if s.summary != nil {
		return s.summary, nil
	}
	return &ClientBillingSummary{}, nil
}

type stubClientTokenChannelResolver struct {
	channel *Channel
	err     error
}

func (s *stubClientTokenChannelResolver) GetChannelForGroup(ctx context.Context, groupID int64) (*Channel, error) {
	if s.err != nil {
		return nil, s.err
	}
	if s.channel == nil {
		return nil, nil
	}
	return s.channel.Clone(), nil
}

func TestClientTokenBillingServiceGetBillingSummary(t *testing.T) {
	repo := &stubClientTokenWalletRepo{
		summary: &ClientBillingSummary{
			RemainingMilliTokens: 1234500,
			RechargedMilliTokens: 2000000,
			ConsumedMilliTokens:  765500,
		},
	}

	svc := NewClientTokenBillingService(repo, nil)
	summary, err := svc.GetBillingSummary(context.Background(), 42)

	require.NoError(t, err)
	require.NotNil(t, summary)
	require.Equal(t, int64(1234500), summary.RemainingMilliTokens)
	require.Equal(t, int64(2000000), summary.RechargedMilliTokens)
	require.Equal(t, int64(765500), summary.ConsumedMilliTokens)
	require.Equal(t, 1234.5, summary.RemainingTokens)
	require.Equal(t, 2000.0, summary.RechargedTokens)
	require.Equal(t, 765.5, summary.ConsumedTokens)
}

func TestClientTokenBillingServiceCheckAccessUsesTokenWallet(t *testing.T) {
	repo := &stubClientTokenWalletRepo{balanceMilli: 1000}
	resolver := &stubClientTokenChannelResolver{
		channel: &Channel{
			ID:                   9,
			Status:               StatusActive,
			SettlementUnit:       SettlementUnitToken,
			TokenInputRatioMilli: 1000,
		},
	}

	svc := NewClientTokenBillingService(repo, resolver)
	usesToken, allowed, err := svc.CheckAccess(context.Background(), 7, 15)

	require.NoError(t, err)
	require.True(t, usesToken)
	require.True(t, allowed)
}

func TestClientTokenBillingServiceCheckAccessFallsBackForMoneyChannel(t *testing.T) {
	repo := &stubClientTokenWalletRepo{balanceMilli: 0}
	resolver := &stubClientTokenChannelResolver{
		channel: &Channel{
			ID:             9,
			Status:         StatusActive,
			SettlementUnit: SettlementUnitMoney,
		},
	}

	svc := NewClientTokenBillingService(repo, resolver)
	usesToken, allowed, err := svc.CheckAccess(context.Background(), 7, 15)

	require.NoError(t, err)
	require.False(t, usesToken)
	require.False(t, allowed)
}

func TestClientTokenBillingServiceCreditRedeemCode(t *testing.T) {
	repo := &stubClientTokenWalletRepo{
		creditResult: &ClientTokenWalletBalance{
			UserID:                    8,
			ChannelID:                 11,
			BalanceMilliTokens:        100000000000,
			TotalRechargedMilliTokens: 100000000000,
		},
	}
	resolver := &stubClientTokenChannelResolver{
		channel: &Channel{
			ID:             11,
			Status:         StatusActive,
			SettlementUnit: SettlementUnitToken,
		},
	}

	svc := NewClientTokenBillingService(repo, resolver)
	result, err := svc.CreditRedeemCode(context.Background(), 8, 21, 100000000, "redeem-code-1")

	require.NoError(t, err)
	require.NotNil(t, result)
	require.Equal(t, 1, repo.creditCalls)
	require.Equal(t, int64(8), repo.creditUserID)
	require.Equal(t, int64(11), repo.creditChannelID)
	require.Equal(t, int64(100000000000), repo.creditAmountMilli)
	require.Equal(t, TokenLedgerSourceRedeemCode, repo.creditSourceType)
	require.Equal(t, int64(100000000000), result.BalanceMilliTokens)
}

func TestClientTokenBillingServiceUpdateAdminTokenBalanceCreditsWallet(t *testing.T) {
	repo := &stubClientTokenWalletRepo{
		creditResult: &ClientTokenWalletBalance{
			UserID:                    8,
			ChannelID:                 11,
			BalanceMilliTokens:        150000000000,
			TotalRechargedMilliTokens: 150000000000,
		},
		summary: &ClientBillingSummary{
			RemainingMilliTokens: 150000000000,
			RechargedMilliTokens: 150000000000,
			ConsumedMilliTokens:  0,
		},
	}
	resolver := &stubClientTokenChannelResolver{
		channel: &Channel{
			ID:             11,
			Status:         StatusActive,
			SettlementUnit: SettlementUnitToken,
		},
	}

	svc := NewClientTokenBillingService(repo, resolver)
	summary, err := svc.UpdateAdminTokenBalance(context.Background(), 8, 21, 150000000, "add", "manual-topup")

	require.NoError(t, err)
	require.NotNil(t, summary)
	require.Equal(t, 1, repo.creditCalls)
	require.Equal(t, TokenLedgerSourceAdminAdjust, repo.creditSourceType)
	require.Equal(t, "add:manual-topup", repo.creditSourceID)
	require.Equal(t, 150000000.0, summary.RemainingTokens)
}

func TestClientTokenBillingServiceUpdateAdminTokenBalanceRejectsOverdraft(t *testing.T) {
	repo := &stubClientTokenWalletRepo{
		balanceMilli: 5000,
	}
	resolver := &stubClientTokenChannelResolver{
		channel: &Channel{
			ID:             11,
			Status:         StatusActive,
			SettlementUnit: SettlementUnitToken,
		},
	}

	svc := NewClientTokenBillingService(repo, resolver)
	summary, err := svc.UpdateAdminTokenBalance(context.Background(), 8, 21, 10, "subtract", "")

	require.Nil(t, summary)
	require.ErrorIs(t, err, ErrInsufficientTokenBalance)
	require.Equal(t, 0, repo.debitCalls)
}
