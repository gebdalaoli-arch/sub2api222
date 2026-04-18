import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'

import DesktopUpdatesView from '../DesktopUpdatesView.vue'

const { listReleases } = vi.hoisted(() => ({
  listReleases: vi.fn().mockResolvedValue({
    items: [],
    total: 0,
    page: 1,
    page_size: 20,
    pages: 0
  })
}))

vi.mock('@/api/admin', () => ({
  adminAPI: {
    desktopUpdates: {
      listReleases
    }
  }
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    showError: vi.fn()
  })
}))

vi.mock('vue-i18n', async () => {
  const actual = await vi.importActual<typeof import('vue-i18n')>('vue-i18n')
  return {
    ...actual,
    useI18n: () => ({
      t: (_key: string, fallback?: string) => fallback ?? _key
    })
  }
})

describe('DesktopUpdatesView', () => {
  it('renders desktop update center actions', () => {
    const wrapper = mount(DesktopUpdatesView, {
      global: {
        stubs: {
          AppLayout: { template: '<div><slot /></div>' },
          TablePageLayout: { template: '<div><slot name="filters" /><slot name="table" /><slot name="pagination" /></div>' },
          BaseDialog: { template: '<div><slot /><slot name="footer" /></div>' },
          ConfirmDialog: true,
          DataTable: { template: '<div><slot /></div>' },
          Pagination: true,
          EmptyState: true,
          Icon: true
        }
      }
    })

    expect(wrapper.text()).toContain('桌面更新中心')
    expect(wrapper.text()).toContain('创建版本')
    expect(wrapper.text()).toContain('公告瀑布')
  })
})
