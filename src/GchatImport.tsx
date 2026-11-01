import { useEffect, useRef, useState } from "react";
import { api, errorText, type ChatMessage, type ChatSpace } from "./api";
import DateRangePicker from "./DateRangePicker";
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

/** The picked messages as report text; the send time tells the AI which day a message is about. */
export function chatReport(messages: ChatMessage[]): string {
  return messages.map((m) => `[Gửi lúc ${sentAt(m.create_time)}]\n${m.text.trim()}`).join("\n\n");
}

interface Props {
  onInsert: (text: string) => void;
  onClose: () => void;
  onOpenSettings: () => void;
}

/** Picks the user's own report messages from a Google Chat group. */
export default function GchatImport({ onInsert, onClose, onOpenSettings }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [connected, setConnected] = useState<boolean | null>(null);
  const [spaces, setSpaces] = useState<ChatSpace[] | null>(null);
  const [space, setSpace] = useState(remembered);
  const [from, setFrom] = useState(() => {
    const d = parseIso(todayIso());
    d.setDate(d.getDate() - 6);
    return isoOf(d);
  });
  const [to, setTo] = useState(todayIso);
  const [messages, setMessages] = useState<ChatMessage[] | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
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
    <dialog
      ref={ref}
      className="confirm-dialog gchat-dialog"
      aria-labelledby="gchat-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
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
        <button className="btn ghost" onClick={onClose}>
          Huỷ
        </button>
        {connected === false ? (
          <button className="btn primary" onClick={onOpenSettings}>
            Mở Cài đặt
          </button>
        ) : (
          <button className="btn primary" disabled={chosen.length === 0} onClick={() => onInsert(chatReport(chosen))}>
            {chosen.length > 0 ? `Đưa ${chosen.length} tin vào báo cáo` : "Đưa vào báo cáo"}
          </button>
        )}
      </div>
    </dialog>
  );
}
