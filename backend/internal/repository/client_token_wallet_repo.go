package repository

import (
	"context"
	"database/sql"
	"fmt"

	dbent "github.com/Wei-Shaw/sub2api/ent"
	"github.com/Wei-Shaw/sub2api/internal/service"
)

type clientTokenWalletRepository struct {
	db *sql.DB
}

func NewClientTokenWalletRepository(_ *dbent.Client, sqlDB *sql.DB) service.ClientTokenWalletRepository {
	return &clientTokenWalletRepository{db: sqlDB}
}

func (r *clientTokenWalletRepository) Credit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	return r.applyDelta(ctx, userID, channelID, amountMilli, 0, sourceType, sourceID)
}

func (r *clientTokenWalletRepository) Debit(ctx context.Context, userID, channelID int64, amountMilli int64, sourceType, sourceID string) (*service.ClientTokenWalletBalance, error) {
	return r.applyDelta(ctx, userID, channelID, 0, amountMilli, sourceType, sourceID)
}

func (r *clientTokenWalletRepository) GetBalance(ctx context.Context, userID, channelID int64) (int64, error) {
	if r == nil || r.db == nil {
		return 0, nil
	}
	var balance int64
	err := r.db.QueryRowContext(ctx, `
		SELECT balance_milli_tokens
		FROM client_token_wallets
		WHERE user_id = $1 AND channel_id = $2
	`, userID, channelID).Scan(&balance)
	if err == sql.ErrNoRows {
		return 0, nil
	}
	if err != nil {
		return 0, err
	}
	return balance, nil
}

func (r *clientTokenWalletRepository) GetSummary(ctx context.Context, userID int64) (*service.ClientBillingSummary, error) {
	if r == nil || r.db == nil {
		return &service.ClientBillingSummary{TokenUnit: "token"}, nil
	}
	summary := &service.ClientBillingSummary{TokenUnit: "token"}
	err := r.db.QueryRowContext(ctx, `
		SELECT
			COALESCE(SUM(balance_milli_tokens), 0),
			COALESCE(SUM(total_recharged_milli_tokens), 0),
			COALESCE(SUM(total_consumed_milli_tokens), 0)
		FROM client_token_wallets
		WHERE user_id = $1
	`, userID).Scan(&summary.RemainingMilliTokens, &summary.RechargedMilliTokens, &summary.ConsumedMilliTokens)
	if err != nil {
		return nil, err
	}
	return summary, nil
}

func (r *clientTokenWalletRepository) applyDelta(ctx context.Context, userID, channelID int64, creditMilli, debitMilli int64, sourceType, sourceID string) (_ *service.ClientTokenWalletBalance, err error) {
	if r == nil || r.db == nil {
		return nil, fmt.Errorf("client token wallet repository db is nil")
	}

	tx, err := r.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer func() {
		if tx != nil {
			_ = tx.Rollback()
		}
	}()

	if _, err := tx.ExecContext(ctx, `
		INSERT INTO client_token_wallets (
			user_id, channel_id, balance_milli_tokens, total_recharged_milli_tokens, total_consumed_milli_tokens
		) VALUES ($1, $2, 0, 0, 0)
		ON CONFLICT (user_id, channel_id) DO NOTHING
	`, userID, channelID); err != nil {
		return nil, err
	}

	current := &service.ClientTokenWalletBalance{}
	if err := tx.QueryRowContext(ctx, `
		SELECT user_id, channel_id, balance_milli_tokens, total_recharged_milli_tokens, total_consumed_milli_tokens
		FROM client_token_wallets
		WHERE user_id = $1 AND channel_id = $2
		FOR UPDATE
	`, userID, channelID).Scan(
		&current.UserID,
		&current.ChannelID,
		&current.BalanceMilliTokens,
		&current.TotalRechargedMilliTokens,
		&current.TotalConsumedMilliTokens,
	); err != nil {
		return nil, err
	}

	current.BalanceMilliTokens += creditMilli - debitMilli
	current.TotalRechargedMilliTokens += creditMilli
	current.TotalConsumedMilliTokens += debitMilli

	if _, err := tx.ExecContext(ctx, `
		UPDATE client_token_wallets
		SET balance_milli_tokens = $1,
			total_recharged_milli_tokens = $2,
			total_consumed_milli_tokens = $3,
			updated_at = NOW()
		WHERE user_id = $4 AND channel_id = $5
	`, current.BalanceMilliTokens, current.TotalRechargedMilliTokens, current.TotalConsumedMilliTokens, userID, channelID); err != nil {
		return nil, err
	}

	if _, err := tx.ExecContext(ctx, `
		INSERT INTO client_token_wallet_ledgers (
			user_id, channel_id, source_type, source_id, credit_milli_tokens, debit_milli_tokens, balance_after_milli_tokens
		) VALUES ($1, $2, $3, $4, $5, $6, $7)
	`, userID, channelID, sourceType, sourceID, creditMilli, debitMilli, current.BalanceMilliTokens); err != nil {
		return nil, err
	}

	if err := tx.Commit(); err != nil {
		return nil, err
	}
	tx = nil
	return current, nil
}
