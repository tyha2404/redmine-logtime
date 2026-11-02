import { useEffect, useRef } from "react";
import type { Settings } from "./api";
import PageHeader from "./PageHeader";
import ReportComposer, { type ComposerHandle } from "./ReportComposer";

interface Props {
  settings: Settings;
  /** Opened from the calendar's "fill with a report": focus the report box. */
  focusReport: boolean;
  onOpenHistory: () => void;
  onOpenSettings: () => void;
}

/** "Ghi logtime": a work report, analysed, checked and written to Redmine. */
export default function ReportPage({ settings, focusReport, onOpenHistory, onOpenSettings }: Props) {
  const composer = useRef<ComposerHandle>(null);

  useEffect(() => {
    if (focusReport) composer.current?.focus();
  }, [focusReport]);

  return (
    <div className="report-page page-pad">
      <PageHeader
        title="Ghi logtime"
        description="Nhập báo cáo, kiểm tra công việc và số giờ, rồi xác nhận ghi lên Redmine."
      />
      <ReportComposer
        ref={composer}
        settings={settings}
        onOpenSettings={onOpenSettings}
        onOpenHistory={onOpenHistory}
      />
    </div>
  );
}
