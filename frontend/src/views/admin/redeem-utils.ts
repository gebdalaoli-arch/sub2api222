import type { RedeemCodeType } from '@/types'

export function redeemTypeNeedsGroup(type: RedeemCodeType): boolean {
  return type === 'subscription' || type === 'token'
}

export function redeemTypeUsesTokenValue(type: RedeemCodeType): boolean {
  return type === 'token'
}

export function redeemTypeUsesValidityDays(type: RedeemCodeType): boolean {
  return type === 'subscription'
}

export function formatAdminRedeemValue(
  row: {
    type: RedeemCodeType
    value: number
    validity_days?: number
  },
  labels?: {
    daysLabel?: string
    tokenSuffix?: string
    currencySymbol?: string
  }
): string {
  const daysLabel = labels?.daysLabel ?? 'days'
  const tokenSuffix = labels?.tokenSuffix ?? 'Token'
  const currencySymbol = labels?.currencySymbol ?? '$'

  if (row.type === 'balance') {
    return `${currencySymbol}${row.value.toFixed(2)}`
  }

  if (row.type === 'subscription') {
    return `${row.validity_days || 30} ${daysLabel}`
  }

  if (row.type === 'token') {
    return `${formatPlainNumber(row.value)} ${tokenSuffix}`
  }

  return formatPlainNumber(row.value)
}

function formatPlainNumber(value: number): string {
  if (!Number.isFinite(value)) {
    return '0'
  }
  if (Number.isInteger(value)) {
    return String(value)
  }
  return value.toFixed(2).replace(/\.?0+$/, '')
}
