import { describe, expect, it } from 'vitest'

import {
  formatAdminRedeemValue,
  redeemTypeNeedsGroup,
  redeemTypeUsesTokenValue
} from '../redeem-utils'

describe('redeem utils', () => {
  it('treats token redeem codes as group-bound token value', () => {
    expect(redeemTypeNeedsGroup('token')).toBe(true)
    expect(redeemTypeUsesTokenValue('token')).toBe(true)
  })

  it('formats token redeem codes with token suffix', () => {
    expect(formatAdminRedeemValue({ type: 'token', value: 100000000 })).toBe('100000000 Token')
  })

  it('keeps subscription redeem values on validity days', () => {
    expect(formatAdminRedeemValue({ type: 'subscription', validity_days: 30, value: 0 })).toBe(
      '30 days'
    )
  })
})
