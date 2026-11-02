import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText } from "./api";
import { inTauri } from "./platform";

/**
 * GitHub repo of the release channel ("owner/name"), baked in by the release
 * workflow (VITE_GITHUB_REPO). Local builds have none and skip the update check.
 */
const MOCK = import.meta.env.VITE_MOCK === "1";
const REPO = import.meta.env.VITE_GITHUB_REPO ?? (MOCK ? "tyha2404/redmine-logtime" : "");

export const APP_VERSION = __APP_VERSION__;

const API = REPO ? `https://api.github.com/repos/${REPO}` : null;

/** Checked again in the background while the app runs. */
const RECHECK_MS = 6 * 60 * 60 * 1000;

/** The one-line installer; re-running it updates in place. */
export const INSTALL_COMMAND = REPO
  ? `curl -fsSL https://raw.githubusercontent.com/${REPO}/HEAD/scripts/install.sh | bash`
  : null;

export interface UpdateState {
  /** Newest released version, once known. */
  latest: string | null;
  /** True when `latest` is newer than this build. */
  available: boolean;
  /** This build can download and install `latest` itself (a signed release for this platform). */
  canInstall: boolean;
  /** "What's new" in `latest`, when the release has notes. */
  notes: string | null;
  checking: boolean;
  /** When the last check finished (ms). */
  checkedAt: number | null;
  /** Set when the check could not run (offline, private project…). */
  error: string | null;
  /** Download progress 0–1 while installing (null total counts as 0), or null. */
  installing: number | null;
  installError: string | null;
}

export interface Update extends UpdateState {
  /** Checks now; resolves with the new state. */
  check: () => Promise<UpdateState>;
  /** Downloads, installs and restarts the app; on failure sets `installError`. */
  install: () => Promise<void>;
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

/** The latest GitHub release's version, for builds that cannot update themselves. */
async function githubLatest(): Promise<string | null> {
  if (!API) return null;
  const r = await fetch(`${API}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
  // 404: the repo has no release yet.
  if (r.status === 404) return null;
  if (!r.ok)
    throw new Error(
      r.status === 403 ? "GitHub tạm giới hạn lượt kiểm tra, thử lại sau." : `GitHub trả lỗi ${r.status}.`,
    );
  const body: { tag_name?: string } = await r.json();
  return (body.tag_name ?? "").replace(/^v/, "") || null;
}

type Found = Pick<UpdateState, "latest" | "canInstall" | "notes">;

/** The app's own updater first; the GitHub release list when it cannot run here. */
async function look(): Promise<Found> {
  if (inTauri || MOCK) {
    try {
      const u = await api.checkUpdate();
      return u
        ? { latest: u.version, canInstall: true, notes: u.notes }
        : { latest: APP_VERSION, canInstall: false, notes: null };
    } catch {
      // No signed release for this platform, or offline: try GitHub below.
    }
  }
  return { latest: await githubLatest(), canInstall: false, notes: null };
}

/** One instance for the whole app (`App.tsx`), passed down to where it is shown. */
export function useUpdate(): Update {
  const [state, setState] = useState<UpdateState>({
    latest: null,
    available: false,
    canInstall: false,
    notes: null,
    checking: !!API || inTauri || MOCK,
    checkedAt: null,
    error: null,
    installing: null,
    installError: null,
  });
  const inFlight = useRef<Promise<UpdateState> | null>(null);
  const current = useRef(state);
  current.current = state;

  const check = useCallback(() => {
    inFlight.current ??= (async () => {
      setState((s) => ({ ...s, checking: true, error: null }));
      let next: UpdateState;
      try {
        const found = await look();
        next = {
          ...current.current,
          ...found,
          available: !!found.latest && isNewer(found.latest, APP_VERSION),
          checking: false,
          checkedAt: Date.now(),
          error: null,
        };
      } catch (e) {
        next = { ...current.current, checking: false, checkedAt: Date.now(), error: errorText(e) };
      }
      setState(next);
      inFlight.current = null;
      return next;
    })();
    return inFlight.current;
  }, []);

  const install = useCallback(async () => {
    setState((s) => ({ ...s, installing: 0, installError: null }));
    const unlisten = await api.onUpdateProgress((p) =>
      setState((s) => ({ ...s, installing: p.total ? p.downloaded / p.total : 0 })),
    );
    try {
      await api.installUpdate(); // restarts the app when it succeeds
    } catch (e) {
      setState((s) => ({ ...s, installing: null, installError: errorText(e) }));
    } finally {
      unlisten();
    }
  }, []);

  useEffect(() => {
    if (!API && !inTauri && !MOCK) return;
    check();
    const timer = setInterval(check, RECHECK_MS);
    return () => clearInterval(timer);
  }, [check]);

  return { ...state, check, install };
}
