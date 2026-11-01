import type { ReactNode } from "react";
import { Icon } from "./icons";

/**
 * Every page starts the same way: title and one line of description on the
 * left, the page's controls (navigation, refresh) on the right.
 */
export default function PageHeader(props: { title: string; description?: ReactNode; actions?: ReactNode }) {
  const { title, description, actions } = props;
  return (
    <header className="page-head">
      <div className="page-head-text">
        <h1 className="page-title">{title}</h1>
        {description && <p className="page-description">{description}</p>}
      </div>
      {actions && <div className="page-actions">{actions}</div>}
    </header>
  );
}

/** The header's reload button, the same on every page. */
export function RefreshButton({ loading, onClick }: { loading: boolean; onClick: () => void }) {
  return (
    <button
      className={`nav-btn icon-only refresh${loading ? " is-loading" : ""}`}
      onClick={onClick}
      disabled={loading}
      aria-label={loading ? "Đang tải lại từ Redmine" : "Tải lại từ Redmine"}
      title="Tải lại từ Redmine"
    >
      <Icon name="refresh" />
    </button>
  );
}
