import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ApiRequest, ApiResponse, RuntimeEvent } from "./generated/api";
import type { AppError } from "./generated/core";

export interface Backend {
  call(request: ApiRequest): Promise<ApiResponse>;
  onEvent(callback: (event: RuntimeEvent) => void): Promise<UnlistenFn>;
}

/** True inside the desktop shell. Outside it, development builds use a labelled mock backend. */
export const isNative = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let mockBackend: Promise<Backend> | undefined;
function mock(): Promise<Backend> {
  if (!import.meta.env.DEV) {
    return Promise.reject<Backend>({ code: "internal", message: "Open Onda from the desktop app.", retryable: false });
  }
  mockBackend ??= import("./mock/backend").then((module) => module.createMockBackend());
  return mockBackend;
}

export function call(request: ApiRequest): Promise<ApiResponse> {
  if (isNative) return invoke<ApiResponse>("application_call", { request });
  return mock().then((backend) => backend.call(request));
}

export function onEvent(callback: (event: RuntimeEvent) => void): Promise<UnlistenFn> {
  if (isNative) return listen<RuntimeEvent>("onda://event", ({ payload }) => callback(payload));
  return mock().then((backend) => backend.onEvent(callback));
}

type ResponseType = ApiResponse["type"];
export type Payload<T extends ResponseType> = Extract<ApiResponse, { type: T }> extends { payload: infer P } ? P : undefined;

/** Call and require a specific response shape. Rejections are always `AppError`. */
export async function ask<T extends ResponseType>(request: ApiRequest, type: T): Promise<Payload<T>> {
  let response: ApiResponse;
  try {
    response = await call(request);
  } catch (error) {
    throw toAppError(error);
  }
  if (response.type !== type) throw toAppError(undefined);
  return ("payload" in response ? response.payload : undefined) as Payload<T>;
}

export function toAppError(error: unknown): AppError {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const value = error as { code: unknown; message: unknown; retryable?: unknown };
    return { code: String(value.code) as AppError["code"], message: String(value.message), retryable: value.retryable === true };
  }
  return { code: "internal", message: "Something went wrong. Try again, or export diagnostics from Settings.", retryable: true };
}
