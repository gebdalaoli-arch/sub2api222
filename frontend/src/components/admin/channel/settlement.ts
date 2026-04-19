import type { Channel, SettlementUnit } from '@/api/admin/channels'

export interface ChannelSettlementForm {
  settlement_unit: SettlementUnit
  token_input_ratio_milli: number
  token_output_ratio_milli: number
  token_cache_write_ratio_milli: number
  token_cache_read_ratio_milli: number
}

const DEFAULT_TOKEN_RATIO_MILLI = 1000

export function normalizeTokenRatioMilli(value: number | null | undefined): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return DEFAULT_TOKEN_RATIO_MILLI
  }
  return Math.round(value)
}

export function createDefaultChannelSettlementForm(): ChannelSettlementForm {
  return {
    settlement_unit: 'money',
    token_input_ratio_milli: DEFAULT_TOKEN_RATIO_MILLI,
    token_output_ratio_milli: DEFAULT_TOKEN_RATIO_MILLI,
    token_cache_write_ratio_milli: DEFAULT_TOKEN_RATIO_MILLI,
    token_cache_read_ratio_milli: DEFAULT_TOKEN_RATIO_MILLI
  }
}

export function channelToSettlementForm(channel?: Partial<Channel> | null): ChannelSettlementForm {
  const defaults = createDefaultChannelSettlementForm()
  if (!channel) {
    return defaults
  }
  return {
    settlement_unit: channel.settlement_unit === 'token' ? 'token' : 'money',
    token_input_ratio_milli: normalizeTokenRatioMilli(channel.token_input_ratio_milli),
    token_output_ratio_milli: normalizeTokenRatioMilli(channel.token_output_ratio_milli),
    token_cache_write_ratio_milli: normalizeTokenRatioMilli(channel.token_cache_write_ratio_milli),
    token_cache_read_ratio_milli: normalizeTokenRatioMilli(channel.token_cache_read_ratio_milli)
  }
}

export function applySettlementToChannelRequest(form: ChannelSettlementForm): ChannelSettlementForm {
  return {
    settlement_unit: form.settlement_unit === 'token' ? 'token' : 'money',
    token_input_ratio_milli: normalizeTokenRatioMilli(form.token_input_ratio_milli),
    token_output_ratio_milli: normalizeTokenRatioMilli(form.token_output_ratio_milli),
    token_cache_write_ratio_milli: normalizeTokenRatioMilli(form.token_cache_write_ratio_milli),
    token_cache_read_ratio_milli: normalizeTokenRatioMilli(form.token_cache_read_ratio_milli)
  }
}
