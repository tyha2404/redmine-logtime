import { useEffect, useRef, useState } from "react";
import { RowsSkeleton } from "./Skeleton";
import { api, errorText, type ChatMessage, type ChatSpace } from "./api";
import DateRangePicker from "./DateRangePicker";
import { useModal } from "./useModal";
import { isoOf, pad, parseIso, todayIso } from "./fmt";

const WEEKDAYS = ["Chủ nhật", "Thứ hai", "Thứ ba", "Thứ tư", "Thứ năm", "Thứ sáu", "Thứ bảy"];
const SPACE_KEY = "gchat-space";
/** Same limit as the backend. */
const MAX_DAYS = 62;

function remembered(): string {
  try {
    return localStorage.getItem(SPACE_KEY) ?? "";
  } catch {
    return "";
  }
}

function remember(space: string) {
  try {
    localStorage.setItem(SPACE_KEY, space);
  } catch {
    // Only a convenience.
  }
}

/** "Thứ hai 26/10/2026 17:05", local time. */
function sentAt(ts: string): string {
  const d = new Date(ts);
  return `${WEEKDAYS[d.getDay()]} ${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Each picked message as report text; the send time helps the AI date messages that name no day. */
export function chatBlocks(messages: ChatMessage[]): string[] {
  return messages.map((m) => `[Gửi lúc ${sentAt(m.create_time)}]\n${m.text.trim()}`);
}

const squash = (s: string) => s.replace(/\s+/g, " ").trim();

/**
 * Adds message blocks to the report text, skipping any already in it (the
 * same range fetched twice). Returns the new text and how many were added.
 */
export function appendBlocks(text: string, blocks: string[]): { text: string; added: number } {
  const have = squash(text);
  const fresh = blocks.filter((b) => !have.includes(squash(b)));
  if (fresh.length === 0) return { text, added: 0 };
  const joined = fresh.join("\n\n");
  return { text: text.trim() ? `${text.trimEnd()}\n\n${joined}` : joined, added: fresh.length };
}

/**
 * The user's messages on one day from the remembered group, for the tray's
 * quick match. Null when that cannot run silently (not connected, no group
 * picked yet, no message): the caller opens the dialog instead.
 */
export async function dayChatBlocks(date: string): Promise<string[] | null> {
  const space = remembered();
  if (!space) return null;
  try {
    if (!(await api.gchatStatus()).connected) return null;
    const list = await api.gchatMessages(space, date, date);
    return list.length > 0 ? chatBlocks(list) : null;
  } catch {
    return null;
  }
}

interface Props {
  onInsert: (blocks: string[]) => void;
  onClose: () => void;
  onOpenSettings: () => void;
  /** First range shown; the last 7 days otherwise. */
  from?: string;
  to?: string;
}

/** Picks the user's own report messages from a Google Chat group. */
export default function GchatImport({ onInsert, onClose, onOpenSettings, from: initialFrom, to: initialTo }: Props) {
  const { close, ...modal } = useModal(onClose);
  const [connected, setConnected] = useState<boolean | null>(null);
  const [spaces, setSpaces] = useState<ChatSpace[] | null>(null);
  const [space, setSpace] = useState(remembered);
  const [from, setFrom] = useState(() => {
    if (initialFrom) return initialFrom;
    const d = parseIso(todayIso());
    d.setDate(d.getDate() - 6);
    return isoOf(d);
  });
  const [to, setTo] = useState(() => initialTo ?? todayIso());
  const [messages, setMessages] = useState<ChatMessage[] | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const s = await api.gchatStatus();
        setConnected(s.connected);
        if (!s.connected) return;
        const list = await api.gchatSpaces();
        setSpaces(list);
        setSpace((cur) => (list.some((x) => x.name === cur) ? cur : (list[0]?.name ?? "")));
      } catch (e) {
        setError(errorText(e));
      }
    })();
  }, []);

  // Loads whenever the group or range changes; a newer request wins.
  const seq = useRef(0);
  const load = async () => {
    const mine = ++seq.current;
    setLoading(true);
    setError(null);
    try {
      const list = await api.gchatMessages(space, from, to);
      if (mine !== seq.current) return;
      remember(space);
      setMessages(list);
      setPicked(new Set(list.map((m) => m.name)));
    } catch (e) {
      if (mine === seq.current) setError(errorText(e));
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  };

  useEffect(() => {
    if (spaces && space) load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaces, space, from, to]);

  const toggle = (name: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  const chosen = messages?.filter((m) => picked.has(m.name)) ?? [];

  return (
    <dialog {...modal} className="confirm-dialog gchat-dialog" aria-labelledby="gchat-title">
      <h2 id="gchat-title">Lấy báo cáo từ Google Chat</h2>

      {connected === false ? (
        <p>
          Chưa kết nối Google Chat. Vào <strong>Cài đặt → Google Chat</strong> để kết nối trước.
        </p>
      ) : (
        <>
          <p>Chỉ lấy tin nhắn do bạn gửi. Bỏ tick những tin không phải báo cáo.</p>
          <div className="gchat-filters">
            <label className="field">
              <span>Group</span>
              <select value={space} onChange={(e) => setSpace(e.target.value)} disabled={!spaces}>
                {!spaces && <option value="">Đang tải…</option>}
                {spaces?.length === 0 && <option value="">Bạn chưa ở group nào</option>}
                {spaces?.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.display_name}
                  </option>
                ))}
              </select>
            </label>
            <div className="field">
              <span id="gchat-range">Khoảng ngày</span>
              <div className="gchat-range-row" aria-labelledby="gchat-range">
                <DateRangePicker
                  from={from}
                  to={to}
                  maxDays={MAX_DAYS}
                  onChange={(a, b) => {
                    setFrom(a);
                    setTo(b);
                  }}
                />
                <button className="btn" onClick={load} disabled={!space || loading}>
                  {loading ? "Đang tải…" : "Tải lại"}
                </button>
              </div>
            </div>
          </div>

          {!messages && !error && (loading || !spaces) && <RowsSkeleton rows={3} className="is-compact gchat-skel" />}
          {messages &&
            (messages.length === 0 ? (
              <p className="gchat-empty">Không có tin nhắn nào của bạn trong khoảng ngày này.</p>
            ) : (
              <ul className="gchat-messages" aria-busy={loading}>
                {messages.map((m) => (
                  <li key={m.name}>
                    <label className="check">
                      <input type="checkbox" checked={picked.has(m.name)} onChange={() => toggle(m.name)} />
                      <span>
                        <span className="gchat-time">{sentAt(m.create_time)}</span>
                        <span className="gchat-text">{m.text}</span>
                      </span>
                    </label>
                  </li>
                ))}
              </ul>
            ))}
        </>
      )}

      {error && <p className="notice error">{error}</p>}

      <div className="confirm-dialog-actions">
        <button className="btn ghost" onClick={() => close()}>
          Huỷ
        </button>
        {connected === false ? (
          <button className="btn primary" onClick={() => close(onOpenSettings)}>
            Mở Cài đặt
          </button>
        ) : (
          <button
            className="btn primary"
            disabled={chosen.length === 0}
            onClick={() => close(() => onInsert(chatBlocks(chosen)))}
          >
            {chosen.length > 0 ? `Đưa ${chosen.length} tin vào báo cáo` : "Đưa vào báo cáo"}
          </button>
        )}
      </div>
    </dialog>
  );
}
