package service

import (
	"context"
	"fmt"
	"math"
	"strings"

	infraerrors "github.com/Wei-Shaw/sub2api/internal/pkg/errors"
)

const (
	TokenLedgerSourceRedeemCode = "redeem_code"
	TokenLedgerSourceUsage      = "usage"
	TokenLedgerSourceAdminAdjust = "admin_adjust"
)

var (
	ErrTokenSettlementChannelNotFound = infraerrors.BadRequest("TOKEN_SETTLEMENT_CHANNEL_NOT_FOUND", "token settlement channel not found for group")
	ErrInvalidTokenAmount             = infraerrors.BadRequest("INVALID_TOKEN_AMOUNT", "token amount must be greater than 0")
	ErrInvalidTokenOperation          = infraerrors.BadRequest("INVALID_TOKEN_OPERATION", "token operation must be add or subtract")
	ErrInsufficientTokenBalance       = infraerrors.BadRequest("INSUFFICIENT_TOKEN_BALANCE", "insufficient token balance")
)

type ClientTokenWalletBalance struct {
	UserID                    int64 `json:"user_id"`
	ChannelID                 int64 `json:"channel_id"`
	BalanceMilliTokens        int64 `json:"balance_milli_tokens"`
	TotalRechargedMilliTokens int64 `json:"total_recharged_milli_tokens"`
	TotalConsumedMilliTokens  int64 `json:"total_consumed_milli_tokens"`
}

type ClientBillingSummary struct {
	RemainingMilliTokens int64   `json:"remaining_milli_tokens"`
	RechargedMilliTokens int64   `json:"recharged_milli_tokens"`
	ConsumedMilliTokens  int64   `json:"consumed_milli_tokens"`
	RemainingTokens      float64 `json:"remaining_tokens"`
	RechargedTokens      float64 `json:"recharged_tokens"`
	ConsumedTokens       float64 `json:"consumed_tokens"`
	TokenUnit            string  `json:"token_unit"`
}

type ClientTokenWalletRepository interface {
	Credit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*ClientTokenWalletBalance, error)
	Debit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*ClientTokenWalletBalance, error)
	GetBalance(ctx context.Context, userID, channelID int64) (int64, error)
	GetSummary(ctx context.Context, userID int64) (*ClientBillingSummary, error)
}

type ClientTokenChannelResolver interface {
	GetChannelForGroup(ctx context.Context, groupID int64) (*Channel, error)
}

type ClientTokenBillingService struct {
	walletRepo      ClientTokenWalletRepository
	channelResolver ClientTokenChannelResolver
}

func NewClientTokenBillingService(walletRepo ClientTokenWalletRepository, channelResolver ClientTokenChannelResolver) *ClientTokenBillingService {
	return &ClientTokenBillingService{
		walletRepo:      walletRepo,
		channelResolver: channelResolver,
	}
}

func milliTokensFromTokens(tokens float64) int64 {
	return int64(math.Round(tokens * 1000))
}

func tokensFromMilliTokens(tokens int64) float64 {
	return float64(tokens) / 1000
}

func (s *ClientTokenBillingService) GetBillingSummary(ctx context.Context, userID int64) (*ClientBillingSummary, error) {
	if s == nil || s.walletRepo == nil {
		return &ClientBillingSummary{TokenUnit: "token"}, nil
	}
	summary, err := s.walletRepo.GetSummary(ctx, userID)
	if err != nil {
		return nil, fmt.Errorf("get client token billing summary: %w", err)
	}
	if summary == nil {
		summary = &ClientBillingSummary{}
	}
	summary.RemainingTokens = tokensFromMilliTokens(summary.RemainingMilliTokens)
	summary.RechargedTokens = tokensFromMilliTokens(summary.RechargedMilliTokens)
	summary.ConsumedTokens = tokensFromMilliTokens(summary.ConsumedMilliTokens)
	if summary.TokenUnit == "" {
		summary.TokenUnit = "token"
	}
	return summary, nil
}

func (s *ClientTokenBillingService) CheckAccess(ctx context.Context, userID, groupID int64) (bool, bool, error) {
	if s == nil || s.channelResolver == nil || s.walletRepo == nil {
		return false, false, nil
	}
	channel, err := s.channelResolver.GetChannelForGroup(ctx, groupID)
	if err != nil {
		return false, false, fmt.Errorf("resolve token settlement channel: %w", err)
	}
	if channel == nil || !channel.UsesTokenSettlement() {
		return false, false, nil
	}
	balanceMilli, err := s.walletRepo.GetBalance(ctx, userID, channel.ID)
	if err != nil {
		return true, false, fmt.Errorf("get token wallet balance: %w", err)
	}
	return true, balanceMilli > 0, nil
}

func (s *ClientTokenBillingService) CreditRedeemCode(ctx context.Context, userID, groupID int64, tokenAmount float64, code string) (*ClientTokenWalletBalance, error) {
	if s == nil || s.walletRepo == nil || s.channelResolver == nil {
		return nil, ErrTokenSettlementChannelNotFound
	}
	amountMilli := milliTokensFromTokens(tokenAmount)
	if amountMilli <= 0 {
		return nil, ErrInvalidTokenAmount
	}
	channel, err := s.channelResolver.GetChannelForGroup(ctx, groupID)
	if err != nil {
		return nil, fmt.Errorf("resolve token settlement channel: %w", err)
	}
	if channel == nil || !channel.UsesTokenSettlement() {
		return nil, ErrTokenSettlementChannelNotFound
	}
	balance, err := s.walletRepo.Credit(ctx, userID, channel.ID, amountMilli, TokenLedgerSourceRedeemCode, code)
	if err != nil {
		return nil, fmt.Errorf("credit token wallet: %w", err)
	}
	return balance, nil
}

func (s *ClientTokenBillingService) UpdateAdminTokenBalance(ctx context.Context, userID, groupID int64, tokenAmount float64, operation, notes string) (*ClientBillingSummary, error) {
	if s == nil || s.walletRepo == nil || s.channelResolver == nil {
		return nil, ErrTokenSettlementChannelNotFound
	}
	amountMilli := milliTokensFromTokens(tokenAmount)
	if amountMilli <= 0 {
		return nil, ErrInvalidTokenAmount
	}
	channel, err := s.channelResolver.GetChannelForGroup(ctx, groupID)
	if err != nil {
		return nil, fmt.Errorf("resolve token settlement channel: %w", err)
	}
	if channel == nil || !channel.UsesTokenSettlement() {
		return nil, ErrTokenSettlementChannelNotFound
	}

	sourceID := buildAdminAdjustSourceID(operation, notes)
	switch operation {
	case "add":
		if _, err := s.walletRepo.Credit(ctx, userID, channel.ID, amountMilli, TokenLedgerSourceAdminAdjust, sourceID); err != nil {
			return nil, fmt.Errorf("credit token wallet: %w", err)
		}
	case "subtract":
		balanceMilli, err := s.walletRepo.GetBalance(ctx, userID, channel.ID)
		if err != nil {
			return nil, fmt.Errorf("get token wallet balance: %w", err)
		}
		if balanceMilli < amountMilli {
			return nil, ErrInsufficientTokenBalance
		}
		if _, err := s.walletRepo.Debit(ctx, userID, channel.ID, amountMilli, TokenLedgerSourceAdminAdjust, sourceID); err != nil {
			return nil, fmt.Errorf("debit token wallet: %w", err)
		}
	default:
		return nil, ErrInvalidTokenOperation
	}

	return s.GetBillingSummary(ctx, userID)
}

func buildAdminAdjustSourceID(operation, notes string) string {
	trimmed := strings.TrimSpace(notes)
	if trimmed != "" {
		return fmt.Sprintf("%s:%s", operation, trimmed)
	}
	return operation
}

func ResolveUsageTokenDebit(ctx context.Context, resolver ClientTokenChannelResolver, groupID *int64, inputTokens, outputTokens, cacheCreateTokens, cacheReadTokens int) (int64, int64, bool, error) {
	if resolver == nil || groupID == nil {
		return 0, 0, false, nil
	}
	if channelService, ok := resolver.(*ChannelService); ok && channelService == nil {
		return 0, 0, false, nil
	}
	channel, err := resolver.GetChannelForGroup(ctx, *groupID)
	if err != nil {
		return 0, 0, false, fmt.Errorf("resolve token settlement channel: %w", err)
	}
	if channel == nil || !channel.UsesTokenSettlement() {
		return 0, 0, false, nil
	}
	return channel.ID, channel.ComputeTokenDebitMilli(inputTokens, outputTokens, cacheCreateTokens, cacheReadTokens), true, nil
}
