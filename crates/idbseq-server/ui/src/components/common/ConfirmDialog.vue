<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import FormField from '@/components/common/FormField.vue';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

// A confirmation, optionally requiring the user to type a name first (the
// same safeguard as the CLI). Enter confirms; Escape or Cancel closes.
const open = defineModel<boolean>('open', { required: true });
const props = defineProps<{
  title: string;
  paragraphs?: string[];
  confirmLabel: string;
  typeToConfirm?: string;
}>();
const emit = defineEmits<{ confirm: [] }>();

const typed = ref('');
const isReady = computed(() => !props.typeToConfirm || typed.value === props.typeToConfirm);

watch(open, (isOpen) => {
  if (isOpen) typed.value = '';
});

function confirm(): void {
  if (!isReady.value) return;
  open.value = false;
  emit('confirm');
}
</script>

<template>
  <AlertDialog v-model:open="open">
    <AlertDialogContent>
      <form class="grid gap-4" @submit.prevent="confirm">
        <AlertDialogHeader>
          <AlertDialogTitle>{{ props.title }}</AlertDialogTitle>
          <AlertDialogDescription v-for="(text, index) in props.paragraphs ?? []" :key="index">{{ text }}</AlertDialogDescription>
        </AlertDialogHeader>
        <FormField v-if="props.typeToConfirm" v-slot="{ id }" :label="`Type ${props.typeToConfirm} to confirm`">
          <Input :id="id" v-model="typed" autocomplete="off" spellcheck="false" class="font-mono" />
        </FormField>
        <AlertDialogFooter>
          <AlertDialogCancel type="button">Cancel</AlertDialogCancel>
          <Button type="submit" variant="destructive" :disabled="!isReady">{{ props.confirmLabel }}</Button>
        </AlertDialogFooter>
      </form>
    </AlertDialogContent>
  </AlertDialog>
</template>
