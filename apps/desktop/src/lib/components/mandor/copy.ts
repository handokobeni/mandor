// Salin teks ke clipboard dan beri tahu lewat toast.

import { toast } from 'svelte-sonner';

export async function copyText(text: string, title: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(title, { description: text });
  } catch {
    toast.error('Could not copy to the clipboard');
  }
}
