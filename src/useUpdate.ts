import { useEffect, useState } from "react";

/**
 * GitHub repo of the release channel ("owner/name"), baked in by the release
 * workflow (VITE_GITHUB_REPO). Local builds have none and skip the update check.
 */
const MOCK = import.meta.env.VITE_MOCK === "1";
const REPO = import.meta.env.VITE_GITHUB_REPO ?? (MOCK ? "tyha2404/redmine-logtime" : "");

export const APP_VERSION = __APP_VERSION__;

const API = REPO ? `https://api.github.com/repos/${REPO}` : null;

/** The one-line installer; re-running it updates in place. */
export const INSTALL_COMMAND = REPO
  ? `curl -fsSL https://raw.githubusercontent.com/${REPO}/HEAD/scripts/install.sh | bash`
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
  cached ??= fetch(`${API}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => {
      // 404: the repo has no release yet.
      if (r.status === 404) return null;
      if (!r.ok) throw new Error(r.status === 403 ? "GitHub tạm giới hạn lượt kiểm tra, thử lại sau." : `GitHub trả lỗi ${r.status}.`);
      return r.json();
    })
    .then((r: { tag_name?: string } | null) => (r?.tag_name ?? "").replace(/^v/, "") || null)
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
