import { createRouter, createWebHashHistory } from 'vue-router';

import { session } from './session';
import AppShell from './views/AppShell.vue';
import AuditView from './views/AuditView.vue';
import CollectionView from './views/CollectionView.vue';
import CollectionsIndex from './views/CollectionsIndex.vue';
import CollectionsLayout from './views/CollectionsLayout.vue';
import KeysView from './views/KeysView.vue';
import LocalView from './views/LocalView.vue';
import LoginView from './views/LoginView.vue';
import SetupView from './views/SetupView.vue';
import StatusView from './views/StatusView.vue';

declare module 'vue-router' {
  interface RouteMeta {
    isPublic?: boolean;
  }
}

export const router = createRouter({
  // Hash URLs (/ui/#/collections), so the server never needs a fallback route.
  history: createWebHashHistory(),
  routes: [
    { path: '/login', name: 'login', component: LoginView, meta: { isPublic: true } },
    { path: '/setup', name: 'setup', component: SetupView, meta: { isPublic: true } },
    { path: '/local', name: 'local', component: LocalView, meta: { isPublic: true } },
    {
      path: '/',
      component: AppShell,
      children: [
        { path: '', redirect: { name: 'collections' } },
        {
          path: 'collections',
          component: CollectionsLayout,
          children: [
            { path: '', name: 'collections', component: CollectionsIndex },
            { path: ':name', name: 'collection', component: CollectionView, props: true },
          ],
        },
        { path: 'keys', name: 'keys', component: KeysView },
        { path: 'audit', name: 'audit', component: AuditView },
        { path: 'status', name: 'status', component: StatusView },
      ],
    },
    { path: '/:unknown(.*)*', redirect: { name: 'collections' } },
  ],
});

const SERVER_ONLY = new Set(['keys', 'audit']);

router.beforeEach((to) => {
  // A console on a file: no accounts, keys or audit log; a lost link cannot sign in again.
  if (session.file !== null && session.token === null && to.name !== 'local') return { name: 'local' };
  if (session.file !== null && SERVER_ONLY.has(String(to.name))) return { name: 'collections' };
  if (!to.meta.isPublic && session.token === null) {
    const isDefault = to.name === 'collections' || to.path === '/';
    return { name: 'login', query: isDefault ? {} : { redirect: to.fullPath } };
  }
  if ((to.name === 'login' || to.name === 'setup') && session.token !== null) return { name: 'collections' };
  return true;
});

/** Where to go after signing in: the requested in-app path, never elsewhere. */
export function safeRedirect(value: unknown): string {
  return typeof value === 'string' && value.startsWith('/') && !value.startsWith('//')
    ? value
    : '/collections';
}
