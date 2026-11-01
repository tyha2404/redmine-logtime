/// <reference types="vite/client" />

/** From src-tauri/tauri.conf.json at build time. */
declare const __APP_VERSION__: string;

interface ImportMetaEnv {
  readonly VITE_MOCK?: string;
  /** Set by the release workflow: GitHub repo ("owner/name") of releases and the installer. */
  readonly VITE_GITHUB_REPO?: string;
}
