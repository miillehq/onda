import { signal } from "@preact/signals";
import { toAppError } from "../api";
import type { AppError } from "../generated/core";
import { presentError } from "./errors";

export interface Toast {
  id: number;
  tone: "info" | "ok" | "error";
  title: string;
  body?: string;
  action?: { label: string; run: () => void };
}
export const toasts = signal<Toast[]>([]);
let toastId = 0;

export function toast(value: Omit<Toast, "id">, ms = 5200): void {
  const id = (toastId += 1);
  toasts.value = [...toasts.value.slice(-2), { ...value, id }];
  window.setTimeout(() => dismissToast(id), ms);
}

export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter((item) => item.id !== id);
}

export function toastError(error: unknown, title?: string): AppError {
  const value = toAppError(error);
  if (value.code !== "cancelled") toast({ tone: "error", title: title ?? presentError(value).title, body: value.message }, 7000);
  return value;
}
