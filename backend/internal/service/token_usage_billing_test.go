package service

import (
	"context"
	"testing"

	"github.com/Wei-Shaw/sub2api/internal/pkg/pagination"
	"github.com/stretchr/testify/require"
)

type tokenBillingChannelRepoStub struct {
	channel *Channel
}

func (s *tokenBillingChannelRepoStub) Create(ctx context.Context, channel *Channel) error {
	panic("unexpected Create call")
}
func (s *tokenBillingChannelRepoStub) GetByID(ctx context.Context, id int64) (*Channel, error) {
	if s.channel != nil && s.channel.ID == id {
		return s.channel.Clone(), nil
	}
	return nil, ErrChannelNotFound
}
func (s *tokenBillingChannelRepoStub) Update(ctx context.Context, channel *Channel) error {
	panic("unexpected Update call")
}
func (s *tokenBillingChannelRepoStub) Delete(ctx context.Context, id int64) error {
	panic("unexpected Delete call")
}
func (s *tokenBillingChannelRepoStub) List(ctx context.Context, params pagination.PaginationParams, status, search string) ([]Channel, *pagination.PaginationResult, error) {
	panic("unexpected List call")
}
func (s *tokenBillingChannelRepoStub) ListAll(ctx context.Context) ([]Channel, error) {
	if s.channel == nil {
		return []Channel{}, nil
	}
	return []Channel{*s.channel.Clone()}, nil
}
func (s *tokenBillingChannelRepoStub) ExistsByName(ctx context.Context, name string) (bool, error) {
	return false, nil
}
func (s *tokenBillingChannelRepoStub) ExistsByNameExcluding(ctx context.Context, name string, excludeID int64) (bool, error) {
	return false, nil
}
func (s *tokenBillingChannelRepoStub) GetGroupIDs(ctx context.Context, channelID int64) ([]int64, error) {
	return s.channel.GroupIDs, nil
}
func (s *tokenBillingChannelRepoStub) SetGroupIDs(ctx context.Context, channelID int64, groupIDs []int64) error {
	panic("unexpected SetGroupIDs call")
}
func (s *tokenBillingChannelRepoStub) GetChannelIDByGroupID(ctx context.Context, groupID int64) (int64, error) {
	return 0, nil
}
func (s *tokenBillingChannelRepoStub) GetGroupsInOtherChannels(ctx context.Context, channelID int64, groupIDs []int64) ([]int64, error) {
	return nil, nil
}
func (s *tokenBillingChannelRepoStub) GetGroupPlatforms(ctx context.Context, groupIDs []int64) (map[int64]string, error) {
	out := make(map[int64]string, len(groupIDs))
	for _, groupID := range groupIDs {
		out[groupID] = PlatformOpenAI
	}
	return out, nil
}
func (s *tokenBillingChannelRepoStub) ListModelPricing(ctx context.Context, channelID int64) ([]ChannelModelPricing, error) {
	return nil, nil
}
func (s *tokenBillingChannelRepoStub) CreateModelPricing(ctx context.Context, pricing *ChannelModelPricing) error {
	panic("unexpected CreateModelPricing call")
}
func (s *tokenBillingChannelRepoStub) UpdateModelPricing(ctx context.Context, pricing *ChannelModelPricing) error {
	panic("unexpected UpdateModelPricing call")
}
func (s *tokenBillingChannelRepoStub) DeleteModelPricing(ctx context.Context, id int64) error {
	panic("unexpected DeleteModelPricing call")
}
func (s *tokenBillingChannelRepoStub) ReplaceModelPricing(ctx context.Context, channelID int64, pricingList []ChannelModelPricing) error {
	panic("unexpected ReplaceModelPricing call")
}

func TestResolveUsageTokenDebit(t *testing.T) {
	groupID := int64(91)
	channelService := NewChannelService(&tokenBillingChannelRepoStub{
		channel: &Channel{
			ID:                        12,
			Name:                      "desktop-openai",
			Status:                    StatusActive,
			GroupIDs:                  []int64{groupID},
			SettlementUnit:            SettlementUnitToken,
			TokenInputRatioMilli:      1000,
			TokenOutputRatioMilli:     2000,
			TokenCacheWriteRatioMilli: 300,
			TokenCacheReadRatioMilli:  100,
		},
	}, nil)

	channelID, debitMilli, usesToken, err := ResolveUsageTokenDebit(context.Background(), channelService, &groupID, 10, 6, 4, 2)

	require.NoError(t, err)
	require.True(t, usesToken)
	require.Equal(t, int64(12), channelID)
	require.Equal(t, int64(23400), debitMilli)
}

func TestBuildUsageBillingCommandUsesTokenDebitInsteadOfBalance(t *testing.T) {
	requestID := "request-token-1"
	usageLog := &UsageLog{
		Model:               "gpt-5.4",
		BillingType:         BillingTypeBalance,
		InputTokens:         10,
		OutputTokens:        6,
		CacheCreationTokens: 4,
		CacheReadTokens:     2,
	}
	params := &postUsageBillingParams{
		Cost: &CostBreakdown{
			ActualCost: 1.23,
			TotalCost:  1.23,
		},
		User:    &User{ID: 601},
		APIKey:  &APIKey{ID: 501},
		Account: &Account{ID: 701},
		TokenSettlement: &TokenSettlementInfo{
			ChannelID:  12,
			DebitMilli: 23400,
		},
	}

	cmd := buildUsageBillingCommand(requestID, usageLog, params)

	require.NotNil(t, cmd)
	require.Equal(t, int64(12), cmd.ChannelID)
	require.Equal(t, int64(23400), cmd.TokenDebitMilli)
	require.Zero(t, cmd.BalanceCost)
}
