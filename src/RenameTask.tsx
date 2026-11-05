import { useState } from "react";
import type { IssueRename, TimeEntry } from "./api";

interface Props {
  /** A logged entry on an issue. */
  entry: TimeEntry;
  onSave: (rename: IssueRename) => void;
  onCancel: () => void;
}

/**
 * Renames the issue a logged entry sits on. That is the only change the app
 * makes to work already on Redmine: hours and comments stay as logged.
 * Saving hands the rename to the confirmation dialog; nothing is sent here.
 */
export default function RenameTask({ entry, onSave, onCancel }: Props) {
  const id = entry.issue!.id;
  const current = entry.issue?.subject ?? "";
  const [subject, setSubject] = useState(current);
  const [error, setError] = useState<string | null>(null);

  const save = (e: React.FormEvent) => {
    e.preventDefault();
    const after = subject.trim();
    if (!after) return setError("Nhập tên task.");
    if (after === current.trim()) return setError("Tên chưa thay đổi.");
    onSave({ issue_id: id, before: current, after });
  };

  return (
    <form className="entry-editor" onSubmit={save} noValidate>
      <label className="field">
        <span>Tên task #{id}</span>
        <input
          value={subject}
          onChange={(e) => {
            setSubject(e.target.value);
            setError(null);
          }}
          autoFocus
        />
        <small className="muted">
          Đổi tên issue trên Redmine; mọi logtime của task này mang tên mới. Số giờ và ghi chú đã ghi giữ nguyên.
        </small>
      </label>
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
      <div className="form-actions">
        <button type="submit" className="btn primary">
          Đổi tên trên Redmine…
        </button>
        <button type="button" className="btn ghost" onClick={onCancel}>
          Huỷ
        </button>
      </div>
    </form>
  );
}
