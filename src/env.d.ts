/// <reference types="vite/client" />

/** From src-tauri/tauri.conf.json at build time. */
declare const __APP_VERSION__: string;

interface ImportMetaEnv {
  readonly VITE_MOCK?: string;
  /** Set by GitLab CI: where releases and the installer live. */
  readonly VITE_GITLAB_URL?: string;
  readonly VITE_GITLAB_PROJECT?: string;
}
