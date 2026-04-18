<template>
  <AppLayout>
    <TablePageLayout>
      <template #filters>
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h1 class="text-2xl font-semibold text-gray-900 dark:text-white">桌面更新中心</h1>
            <p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
              管理 Windows 安装包、更新说明和公告瀑布。
            </p>
          </div>
          <button class="btn btn-primary" @click="openCreateDialog">
            创建版本
          </button>
        </div>
      </template>

      <template #table>
        <div class="mb-4 rounded-xl border border-blue-100 bg-blue-50/70 p-4 text-sm text-blue-900 dark:border-blue-900/40 dark:bg-blue-950/20 dark:text-blue-100">
          <div class="font-medium">公告瀑布</div>
          <div class="mt-1 text-xs opacity-80">
            每个版本可以附带面向客户端展示的公告与更新说明。
          </div>
        </div>

        <DataTable :columns="columns" :data="releases" :loading="loading">
          <template #cell-title="{ row }">
            <div class="min-w-0">
              <div class="font-medium text-gray-900 dark:text-white">{{ row.title }}</div>
              <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                v{{ row.version }} · {{ row.platform }}/{{ row.arch }}
              </div>
            </div>
          </template>

          <template #cell-summary="{ value }">
            <div class="max-w-xl truncate text-sm text-gray-600 dark:text-gray-300">{{ value }}</div>
          </template>

          <template #cell-file_size="{ value }">
            <span class="text-sm text-gray-500 dark:text-gray-400">{{ formatFileSize(value) }}</span>
          </template>
        </DataTable>
      </template>
    </TablePageLayout>

    <BaseDialog :show="showEditDialog" title="版本发布" width="wide" @close="closeEdit">
      <form id="desktop-update-form" class="space-y-4" @submit.prevent="handleSave">
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <label class="input-label">版本号</label>
            <input v-model="form.version" class="input" />
          </div>
          <div>
            <label class="input-label">最低支持版本</label>
            <input v-model="form.minimum_supported_version" class="input" />
          </div>
        </div>

        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <label class="input-label">标题</label>
            <input v-model="form.title" class="input" />
          </div>
          <div>
            <label class="input-label">摘要</label>
            <input v-model="form.summary" class="input" />
          </div>
        </div>

        <div>
          <label class="input-label">更新说明</label>
          <textarea v-model="form.release_notes_markdown" class="input" rows="8"></textarea>
        </div>

        <div>
          <label class="input-label">安装包</label>
          <input ref="packageInput" type="file" accept=".exe" class="input" />
        </div>
      </form>

      <template #footer>
        <div class="flex justify-between gap-3">
          <button class="btn btn-secondary" type="button" @click="closeEdit">
            {{ t('common.cancel') }}
          </button>
          <button class="btn btn-primary" type="submit" form="desktop-update-form" :disabled="saving">
            {{ saving ? t('common.saving') : t('common.save') }}
          </button>
        </div>
      </template>
    </BaseDialog>
  </AppLayout>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import { useAppStore } from '@/stores/app'
import type { Column } from '@/components/common/types'
import type { CreateDesktopReleasePayload, DesktopRelease } from '@/types'

import AppLayout from '@/components/layout/AppLayout.vue'
import TablePageLayout from '@/components/layout/TablePageLayout.vue'
import DataTable from '@/components/common/DataTable.vue'
import BaseDialog from '@/components/common/BaseDialog.vue'

const { t } = useI18n()
const appStore = useAppStore()

const releases = ref<DesktopRelease[]>([])
const loading = ref(false)
const saving = ref(false)
const showEditDialog = ref(false)
const packageInput = ref<HTMLInputElement | null>(null)

const form = reactive({
  version: '',
  platform: 'windows',
  arch: 'x64',
  title: '',
  summary: '',
  release_notes_markdown: '',
  minimum_supported_version: '0.1.0',
  published: true,
  force_update: false,
})

  const columns = computed<Column[]>(() => [
  { key: 'title', label: t('admin.desktopUpdates.columns.title') },
  { key: 'summary', label: t('admin.desktopUpdates.columns.summary') },
  { key: 'file_size', label: t('admin.desktopUpdates.columns.fileSize') },
])

async function loadReleases() {
  try {
    loading.value = true
    const result = await adminAPI.desktopUpdates.listReleases()
    releases.value = result.items
  } catch (error: any) {
    appStore.showError(error?.message || t('admin.desktopUpdates.failedToLoad', '加载桌面更新失败'))
  } finally {
    loading.value = false
  }
}

function openCreateDialog() {
  showEditDialog.value = true
}

function closeEdit() {
  showEditDialog.value = false
}

async function handleSave() {
  const file = packageInput.value?.files?.[0]
  if (!file) {
    appStore.showError(t('admin.desktopUpdates.packageRequired', '请先选择 Windows 安装包'))
    return
  }

  try {
    saving.value = true
    const payload: CreateDesktopReleasePayload = {
      version: form.version,
      platform: form.platform,
      arch: form.arch,
      title: form.title,
      summary: form.summary,
      release_notes_markdown: form.release_notes_markdown,
      announcement_items: [],
      published: form.published,
      force_update: form.force_update,
      minimum_supported_version: form.minimum_supported_version,
      package: file,
    }
    await adminAPI.desktopUpdates.createRelease(payload)
    closeEdit()
    await loadReleases()
  } catch (error: any) {
    appStore.showError(error?.message || t('admin.desktopUpdates.failedToSave', '保存桌面更新失败'))
  } finally {
    saving.value = false
  }
}

function formatFileSize(size: number) {
  if (!size) return '0 B'
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / (1024 * 1024)).toFixed(1)} MB`
}

onMounted(() => {
  void loadReleases()
})
</script>
