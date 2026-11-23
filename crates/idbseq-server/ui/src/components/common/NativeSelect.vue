<script setup lang="ts" generic="T extends string">
// A native <select> in the shadcn look. reka-ui's Select renders an inline
// <style> element, which the console's Content-Security-Policy blocks.
const model = defineModel<T>({ required: true });
defineProps<{ options: ReadonlyArray<{ value: T; label: string }> }>();
</script>

<template>
  <select
    v-model="model"
    class="h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 dark:bg-input/30"
  >
    <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
  </select>
</template>
