<script setup lang="ts">
defineProps<{ crumbs: Array<{ label: string; to?: string }> }>();
</script>

<template>
  <header class="flex min-h-14 shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-b px-6 py-2">
    <nav aria-label="Breadcrumb" class="min-w-0">
      <ol class="flex items-center gap-2 text-lg">
        <li v-for="(crumb, index) in crumbs" :key="index" class="flex min-w-0 items-center gap-2">
          <span v-if="index > 0" class="text-muted-foreground" aria-hidden="true">/</span>
          <RouterLink v-if="crumb.to" :to="crumb.to" class="text-muted-foreground hover:text-foreground">{{ crumb.label }}</RouterLink>
          <h1 v-else class="truncate font-semibold">{{ crumb.label }}</h1>
        </li>
      </ol>
    </nav>
    <slot name="title-extra" />
    <div class="ml-auto flex items-center gap-2">
      <slot name="actions" />
    </div>
  </header>
</template>
