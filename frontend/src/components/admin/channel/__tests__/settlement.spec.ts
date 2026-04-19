import { describe, expect, it } from 'vitest'

import {
  applySettlementToChannelRequest,
  channelToSettlementForm,
  createDefaultChannelSettlementForm
} from '../settlement'

describe('channel settlement helpers', () => {
  it('creates money settlement defaults', () => {
    expect(createDefaultChannelSettlementForm()).toEqual({
      settlement_unit: 'money',
      token_input_ratio_milli: 1000,
      token_output_ratio_milli: 1000,
      token_cache_write_ratio_milli: 1000,
      token_cache_read_ratio_milli: 1000
    })
  })

  it('hydrates token settlement from api channel', () => {
    const form = channelToSettlementForm({
      settlement_unit: 'token',
      token_input_ratio_milli: 1000,
      token_output_ratio_milli: 2500,
      token_cache_write_ratio_milli: 300,
      token_cache_read_ratio_milli: 100
    })

    expect(form).toEqual({
      settlement_unit: 'token',
      token_input_ratio_milli: 1000,
      token_output_ratio_milli: 2500,
      token_cache_write_ratio_milli: 300,
      token_cache_read_ratio_milli: 100
    })
  })

  it('serializes settlement fields for admin api requests', () => {
    expect(applySettlementToChannelRequest({
      settlement_unit: 'token',
      token_input_ratio_milli: 1000,
      token_output_ratio_milli: 2000,
      token_cache_write_ratio_milli: 500,
      token_cache_read_ratio_milli: 100
    })).toEqual({
      settlement_unit: 'token',
      token_input_ratio_milli: 1000,
      token_output_ratio_milli: 2000,
      token_cache_write_ratio_milli: 500,
      token_cache_read_ratio_milli: 100
    })
  })
})
