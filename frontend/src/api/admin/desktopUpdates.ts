import { apiClient } from '../client'
import type { BasePaginationResponse, CreateDesktopReleasePayload, DesktopRelease } from '@/types'

export async function listReleases(page = 1, pageSize = 20): Promise<BasePaginationResponse<DesktopRelease>> {
  const { data } = await apiClient.get<BasePaginationResponse<DesktopRelease>>('/admin/desktop-updates/releases', {
    params: { page, page_size: pageSize },
  })
  return data
}

export async function createRelease(payload: CreateDesktopReleasePayload): Promise<DesktopRelease> {
  const formData = new FormData()
  formData.append('version', payload.version)
  formData.append('platform', payload.platform)
  formData.append('arch', payload.arch)
  formData.append('title', payload.title)
  formData.append('summary', payload.summary)
  formData.append('release_notes_markdown', payload.release_notes_markdown)
  formData.append('published', String(payload.published))
  formData.append('force_update', String(payload.force_update))
  formData.append('minimum_supported_version', payload.minimum_supported_version)
  formData.append('package', payload.package)

  if (payload.announcement_items.length > 0) {
    formData.append('announcement_items', JSON.stringify(payload.announcement_items))
  }

  const { data } = await apiClient.post<DesktopRelease>('/admin/desktop-updates/releases', formData, {
    headers: {
      'Content-Type': 'multipart/form-data'
    }
  })
  return data
}

const desktopUpdatesAPI = {
  listReleases,
  createRelease,
}

export default desktopUpdatesAPI
