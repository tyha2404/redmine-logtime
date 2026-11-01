import { useEffect, useState } from "react";
import AiLog from "./AiLog";
import { api, errorText, type Profile, type Settings, type Tracker } from "./api";
import { fmtTimestamp } from "./fmt";
import { useAiRun } from "./useAiRun";

interface Props {
  settings: Settings;
  trackers: Tracker[];
  /** Copies suggestions into the (unsaved) settings form. */
  onApply: (patch: Partial<Settings>) => void;
}

/** What Claude learned about this Redmine, mainly how to spot BA parent tasks. */
export default function ProfileSection({ settings, trackers, onApply }: Props) {
  const ai = useAiRun();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [notes, setNotes] = useState("");
  const [saved, setSaved] = useState<string | null>(null);

  useEffect(() => {
    api.getProfile().then((p) => {
      setProfile(p);
      setNotes(p?.user_notes ?? "");
    });
  }, []);

  const discover = async () => {
    const p = await ai.run(() => api.discoverProfile());
    if (p) {
      setProfile(p);
      setNotes(p.user_notes);
    }
  };

  const saveNotes = async () => {
    const base: Profile = profile ?? {
      parent_rule: { tracker_ids: [], tracker_names: [], author_ids: [], author_names: [], role_names: [], description: "" },
      subtask_tracker_id: null,
      leave_issue_id: null,
      projects: [],
      naming: "",
      summary: "",
      user_notes: "",
      generated_at: null,
    };
    const next = { ...base, user_notes: notes };
    try {
      await api.saveProfile(next);
      setProfile(next);
      setSaved("Đã lưu.");
    } catch (e) {
      setSaved(errorText(e));
    }
  };

  const trackerName = (id: number | null) => trackers.find((t) => t.id === id)?.name ?? (id ? `#${id}` : "");
  const suggestions: Partial<Settings> = {};
  if (profile?.subtask_tracker_id && profile.subtask_tracker_id !== settings.subtask_tracker_id)
    suggestions.subtask_tracker_id = profile.subtask_tracker_id;

  const rule = profile?.parent_rule;
  const facts: [string, string][] = profile
    ? ([
        ["Tracker task cha", rule!.tracker_names.join(", ")],
        ["Người tạo", [...rule!.role_names, ...rule!.author_names].join(", ")],
        ["Tracker task con", trackerName(profile.subtask_tracker_id)],
        ["Dự án", profile.projects.map((p) => p.name).join(", ")],
      ].filter(([, v]) => v) as [string, string][])
    : [];

  return (
    <>
      <section className="card">
        <h2>Cấu trúc Redmine</h2>
        <p className="hint">
          Claude đọc Redmine (chỉ đọc) để biết task cha do BA tạo trông thế nào và tracker của task con.
        </p>

        {profile?.generated_at ? (
          <div className="profile">
            <p className="profile-rule">{rule!.description || "Chưa xác định được quy tắc task cha."}</p>
            <dl className="profile-facts">
              {facts.map(([k, v]) => (
                <div key={k}>
                  <dt>{k}</dt>
                  <dd>{v}</dd>
                </div>
              ))}
            </dl>
            {(profile.summary || profile.naming) && (
              <p className="profile-summary">{[profile.summary, profile.naming].filter(Boolean).join(" ")}</p>
            )}
          </div>
        ) : (
          !ai.running && <p className="muted">Chưa phân tích. Lưu cài đặt kết nối rồi bấm phân tích (mất khoảng 1–3 phút).</p>
        )}

        <div className="card-foot">
          <AiLog ai={ai} />
          <div className="row wrap">
            <button className="btn" onClick={discover} disabled={ai.running || !settings.redmine_url}>
              {ai.running ? "Đang phân tích…" : profile?.generated_at ? "Phân tích lại" : "Phân tích cấu trúc Redmine"}
            </button>
            {Object.keys(suggestions).length > 0 && (
              <button className="btn" onClick={() => onApply(suggestions)}>
                Dùng gợi ý
                {suggestions.subtask_tracker_id ? `: tracker ${trackerName(suggestions.subtask_tracker_id)}` : ""}
              </button>
            )}
            {profile?.generated_at && <span className="muted small foot-meta">Phân tích lúc {fmtTimestamp(profile.generated_at)}</span>}
          </div>
        </div>
      </section>

      <section className="card">
        <h2>Ghi chú cho Claude</h2>
        <p className="hint">Claude ưu tiên ghi chú này hơn kết quả tự phân tích khi tìm task cha cho báo cáo.</p>
        <label className="field grow-field">
          <span>Ghi chú</span>
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="Ví dụ: Task cha luôn là tracker Feature do BA tạo; dự án ABC đặt tên task con có tiền tố [ABC]."
          />
        </label>
        <div className="card-foot">
          <div className="row">
            <button className="btn" onClick={saveNotes} disabled={notes === (profile?.user_notes ?? "")}>
              Lưu ghi chú
            </button>
            {saved && <span className="notice ok inline">{saved}</span>}
          </div>
        </div>
      </section>
    </>
  );
}
