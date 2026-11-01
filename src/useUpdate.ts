import { useEffect, useState } from "react";

/**
 * Release channel baked in by GitLab CI (VITE_GITLAB_URL, VITE_GITLAB_PROJECT).
 * Local builds have neither, and simply skip the update check.
 */
const MOCK = import.meta.env.VITE_MOCK === "1";
const GITLAB = (import.meta.env.VITE_GITLAB_URL ?? (MOCK ? "https://gitlab.congty.vn" : "")).replace(/\/$/, "");
const PROJECT = import.meta.env.VITE_GITLAB_PROJECT ?? (MOCK ? "42" : "");

export const APP_VERSION = __APP_VERSION__;

const API = GITLAB && PROJECT ? `${GITLAB}/api/v4/projects/${encodeURIComponent(PROJECT)}` : null;

/** The one-line installer; re-running it updates in place. */
export const INSTALL_COMMAND = API
  ? `curl -fsSL ${API}/packages/generic/redmine-logtime/latest/install.sh | bash`
  : null;

export interface UpdateState {
  /** Newest released version, once known. */
  latest: string | null;
  /** True when `latest` is newer than this build. */
  available: boolean;
  checking: boolean;
  /** Set when the check could not run (offline, private project…). */
  error: string | null;
}

let cached: Promise<string | null> | null = null;

function fetchLatest(): Promise<string | null> {
  if (MOCK) return Promise.resolve("0.2.0");
  if (!API) return Promise.resolve(null);
  cached ??= fetch(`${API}/releases?per_page=1`)
    .then((r) => {
      if (!r.ok) throw new Error(r.status === 404 || r.status === 401 ? "Project private, không kiểm tra được bản mới." : `GitLab trả lỗi ${r.status}.`);
      return r.json();
    })
    .then((rs: { tag_name: string }[]) => (rs[0]?.tag_name ?? "").replace(/^v/, "") || null)
    .catch((e) => {
      cached = null;
      throw e;
    });
  return cached;
}

/** a > b for dotted numeric versions. */
export function isNewer(a: string, b: string): boolean {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d > 0;
  }
  return false;
}

export function useUpdate(): UpdateState {
  const [state, setState] = useState<UpdateState>({ latest: null, available: false, checking: !!API || MOCK, error: null });

  useEffect(() => {
    let live = true;
    fetchLatest()
      .then((latest) => live && setState({ latest, available: !!latest && isNewer(latest, APP_VERSION), checking: false, error: null }))
      .catch((e) => live && setState({ latest: null, available: false, checking: false, error: e instanceof Error ? e.message : String(e) }));
    return () => {
      live = false;
    };
  }, []);

  return state;
}
