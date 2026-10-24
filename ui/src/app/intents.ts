import { signal } from "@preact/signals";
import { navigate } from "../lib/router";

/** A one-shot request to open a page's "new item" editor, e.g. from the command palette. */
export const editorIntent = signal<{ page: "dictionary" | "snippets" | "rewrites"; at: number } | null>(null);

export function requestEditor(page: "dictionary" | "snippets" | "rewrites"): void {
  navigate(page);
  editorIntent.value = { page, at: Date.now() };
}

export function consumeEditorIntent(page: "dictionary" | "snippets" | "rewrites"): boolean {
  if (editorIntent.value?.page !== page) return false;
  editorIntent.value = null;
  return true;
}
