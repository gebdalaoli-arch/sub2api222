<template>
  <BaseDialog
    :show="show"
    :title="operation === 'add' ? t('admin.users.depositToken') : t('admin.users.withdrawToken')"
    width="narrow"
    @close="$emit('close')"
  >
    <form v-if="user" id="token-balance-form" @submit.prevent="handleSubmit" class="space-y-5">
      <div class="flex items-center gap-3 rounded-xl bg-gray-50 p-4 dark:bg-dark-700">
        <div class="flex h-10 w-10 items-center justify-center rounded-full bg-primary-100">
          <span class="text-lg font-medium text-primary-700">{{ user.email.charAt(0).toUpperCase() }}</span>
        </div>
        <div class="flex-1">
          <p class="font-medium text-gray-900 dark:text-white">{{ user.email }}</p>
          <p class="text-sm text-gray-500">
            {{ t('admin.users.currentTokenBalance') }}: {{ formatToken(summary?.remaining_tokens ?? 0) }}
          </p>
        </div>
      </div>

      <div v-if="tokenGroupOptions.length > 1">
        <label class="input-label">{{ t('admin.users.tokenGroup') }}</label>
        <Select v-model="form.group_id" :options="tokenGroupOptions" />
      </div>

      <div v-else-if="tokenGroupOptions.length === 1" class="rounded-xl bg-gray-50 p-4 dark:bg-dark-700">
        <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('admin.users.tokenGroup') }}</p>
        <p class="mt-1 text-sm font-medium text-gray-900 dark:text-white">
          {{ tokenGroupOptions[0].label }}
        </p>
      </div>

      <div v-else class="rounded-xl border border-amber-200 bg-amber-50 p-4 text-sm text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
        {{ t('admin.users.noTokenGroupAvailable') }}
      </div>

      <div>
        <label class="input-label">{{ t('admin.users.tokenAmount') }}</label>
        <input v-model.number="form.amount" type="number" min="1" step="1" required class="input" />
      </div>

      <div>
        <label class="input-label">{{ t('admin.users.notes') }}</label>
        <textarea v-model="form.notes" rows="3" class="input"></textarea>
      </div>

      <div v-if="form.amount > 0" class="rounded-xl border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950">
        <div class="flex items-center justify-between text-sm">
          <span class="text-gray-700 dark:text-gray-300">{{ t('admin.users.newTokenBalance') }}:</span>
          <span class="font-bold text-gray-900 dark:text-gray-100">{{ formatToken(calculateNewBalance()) }}</span>
        </div>
      </div>
    </form>

    <template #footer>
      <div class="flex justify-end gap-3">
        <button @click="$emit('close')" class="btn btn-secondary">{{ t('common.cancel') }}</button>
        <button
          type="submit"
          form="token-balance-form"
          :disabled="submitting || !form.amount || !form.group_id || tokenGroupOptions.length === 0"
          class="btn"
          :class="operation === 'add' ? 'bg-emerald-600 text-white' : 'btn-danger'"
        >
          {{ submitting ? t('common.saving') : t('common.confirm') }}
        </button>
      </div>
    </template>
  </BaseDialog>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { adminAPI } from '@/api/admin'
import type { AdminUser } from '@/types'
import type { AdminUserTokenBalanceSummary } from '@/api/admin/users'
import BaseDialog from '@/components/common/BaseDialog.vue'
import Select from '@/components/common/Select.vue'

const props = defineProps<{ show: boolean; user: AdminUser | null; operation: 'add' | 'subtract' }>()
const emit = defineEmits(['close', 'success'])
const { t } = useI18n()
const appStore = useAppStore()

const submitting = ref(false)
const summary = ref<AdminUserTokenBalanceSummary | null>(null)
const tokenGroupOptions = ref<Array<{ value: number; label: string }>>([])
const form = reactive({
  amount: 0,
  group_id: null as number | null,
  notes: ''
})

watch(
  () => props.show,
  async (visible) => {
    if (!visible || !props.user) return
    form.amount = 0
    form.notes = ''
    summary.value = null
    tokenGroupOptions.value = []
    await loadState()
  }
)

async function loadState() {
  if (!props.user) return
  try {
    const [billingSummary, groups, channels] = await Promise.all([
      adminAPI.users.getTokenBalance(props.user.id),
      adminAPI.groups.getAll('openai'),
      adminAPI.channels.list(1, 200, { status: 'active', sort_by: 'created_at', sort_order: 'desc' })
    ])
    summary.value = billingSummary

    const tokenGroupIds = new Set(
      channels.items
        .filter((channel) => channel.settlement_unit === 'token' && channel.status === 'active')
        .flatMap((channel) => channel.group_ids)
    )
    tokenGroupOptions.value = groups
      .filter((group) => group.status === 'active' && tokenGroupIds.has(group.id))
      .map((group) => ({
        value: group.id,
        label: group.name
      }))

    form.group_id = tokenGroupOptions.value[0]?.value ?? null
  } catch (error) {
    console.error('Failed to load token balance state:', error)
    appStore.showError(t('admin.users.failedToLoadTokenBalance'))
  }
}

function calculateNewBalance() {
  const current = summary.value?.remaining_tokens ?? 0
  const next = props.operation === 'add' ? current + form.amount : current - form.amount
  return Math.max(0, next)
}

function formatToken(value: number) {
  if (!Number.isFinite(value) || value <= 0) return `0 ${t('admin.users.tokenUnit')}`
  if (value >= 1000000000000) return `${trim((value / 1000000000000).toFixed(2))}万亿 ${t('admin.users.tokenUnit')}`
  if (value >= 100000000) return `${trim((value / 100000000).toFixed(2))}亿 ${t('admin.users.tokenUnit')}`
  if (value >= 10000) return `${trim((value / 10000).toFixed(2))}万 ${t('admin.users.tokenUnit')}`
  return `${trim(value.toFixed(0))} ${t('admin.users.tokenUnit')}`
}

function trim(value: string) {
  return value.replace(/\.?0+$/, '')
}

async function handleSubmit() {
  if (!props.user || !form.group_id) return
  if (!form.amount || form.amount <= 0) {
    appStore.showError(t('admin.users.tokenAmountRequired'))
    return
  }
  const current = summary.value?.remaining_tokens ?? 0
  if (props.operation === 'subtract' && form.amount > current) {
    appStore.showError(t('admin.users.insufficientTokenBalance'))
    return
  }
  submitting.value = true
  try {
    await adminAPI.users.updateTokenBalance(
      props.user.id,
      form.amount,
      props.operation,
      form.group_id,
      form.notes
    )
    appStore.showSuccess(t('common.success'))
    emit('success')
    emit('close')
  } catch (error: any) {
    console.error('Failed to update token balance:', error)
    appStore.showError(error.response?.data?.detail || t('common.error'))
  } finally {
    submitting.value = false
  }
}
</script>
