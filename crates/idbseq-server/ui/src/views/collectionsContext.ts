import type { InjectionKey } from 'vue';

/** Lets a collection page refresh the sidebar list after a change. */
export const RELOAD_COLLECTIONS: InjectionKey<() => Promise<void>> = Symbol('reload-collections');
