import type { NavAdapter } from '@h-ai/ui'
import type { ManagementStore } from './management-store.svelte.js'
import { createGuide } from '@h-ai/ui/guide'
import * as m from '../paraglide/messages.js'

/** Web/App/Desktop 共用内容；宿主只提供当前页面和平台持久化方式。 */
export function createManagementGuide(store: ManagementStore, platform: 'web' | 'app' | 'desktop') {
  const nav: NavAdapter = {
    get pathname() {
      return `/${store.page}`
    },
    navigate(route) {
      if (route === '/settings')
        return store.navigate('settings')
    },
  }
  return createGuide({
    nav,
    persistKey: 'spacegate-admin-guide',
    persistence: platform === 'web' ? 'cookie' : 'localStorage',
    tours: [{
      id: 'overview',
      title: () => m.guide_overview_title(),
      description: () => m.guide_overview_description(),
      steps: [
        { target: 'admin-content', title: () => m.guide_content_title(), description: () => m.guide_content_description() },
        { target: 'admin-profile', title: () => m.guide_profile_title(), description: () => m.guide_profile_description() },
        { target: 'admin-help', title: () => m.guide_help_title(), description: () => m.guide_help_description() },
      ],
    }, {
      id: 'profile',
      title: () => m.guide_profile_tour_title(),
      description: () => m.guide_profile_tour_description(),
      route: '/settings',
      steps: [
        { target: 'admin-profile-form', title: () => m.guide_profile_title(), description: () => m.guide_profile_form_description() },
        { target: 'admin-preferences', title: () => m.preferences_title(), description: () => m.guide_preferences_description() },
        { target: 'admin-password', title: () => m.guide_password_title(), description: () => m.guide_password_description() },
      ],
    }],
    articles: [{
      id: 'overview',
      title: () => m.guide_overview_title(),
      description: () => m.guide_overview_description(),
      content: () => m.guide_overview_article(),
      tourId: 'overview',
    }, {
      id: 'profile',
      title: () => m.guide_profile_tour_title(),
      description: () => m.guide_profile_tour_description(),
      content: () => m.guide_profile_article(),
      tourId: 'profile',
    }],
  })
}
