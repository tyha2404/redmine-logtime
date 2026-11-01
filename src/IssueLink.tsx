import { api, errorText } from "./api";
import { useToast } from "./toast";

/**
 * "#1234" that opens the issue on Redmine in the system browser. Use it
 * wherever an issue number is shown, so it can be checked at the source.
 */
export default function IssueLink({ id, children }: { id: number; children?: React.ReactNode }) {
  const toast = useToast();
  return (
    <a
      className="issue-link"
      href={`#issue-${id}`}
      title={`Mở #${id} trên Redmine`}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        api.openIssue(id).catch((err) => toast(errorText(err), { tone: "error" }));
      }}
    >
      {children ?? `#${id}`}
    </a>
  );
}

/** A label like "#12 Subject" with its number linked; other text as is. */
export function IssueLabel({ label }: { label: string }) {
  const m = label.match(/^#(\d+)(.*)$/s);
  if (!m) return <>{label}</>;
  return (
    <>
      <IssueLink id={Number(m[1])} />
      {m[2]}
    </>
  );
}
