import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Tauri IPC, or canned data when the UI runs in a plain browser with
 * VITE_MOCK=1 (design reviews, screenshots). The mock is never bundled into
 * production builds.
 */
const MOCK = import.meta.env.VITE_MOCK === "1";

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (MOCK) return (await import("./mock")).mockInvoke(cmd, args) as Promise<T>;
  return tauriInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, cb: EventCallback<T>): Promise<UnlistenFn> {
  if (MOCK) return (await import("./mock")).mockListen(event, cb as EventCallback<unknown>);
  return tauriListen<T>(event, cb);
}

export type { UnlistenFn };
