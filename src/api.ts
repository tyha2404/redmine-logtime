import { invoke, listen, type UnlistenFn } from "./bridge";

// Shapes mirror the Rust structs (serde, snake_case fields).

export interface Settings {
  redmine_url: string;
  accept_invalid_certs: boolean;
  hours_per_day: number;
  /** ISO weekday numbers, Monday = 1 … Sunday = 7. */
  workdays: number[];
  /** YYYY-MM-DD */
  holidays: string[];
  leave_issue_id: number | null;
  leave_activity_id: number | null;
  default_activity_id: number | null;
  subtask_tracker_id: number | null;
  claude_ignore_user_settings: boolean;
}

export interface SettingsView {
  settings: Settings;
  has_api_key: boolean;
}

export interface IdName {
  id: number;
  name: string;
}

export interface User {
  id: number;
  login: string;
  firstname: string;
  lastname: string;
}

export interface Activity {
  id: number;
  name: string;
  is_default: boolean;
  active: boolean;
}

export interface Tracker {
  id: number;
  name: string;
}

export interface Issue {
  id: number;
  project: IdName;
  tracker: IdName;
  status: IdName;
  subject: string;
  parent: { id: number } | null;
  author: IdName;
  assigned_to: IdName | null;
}

export interface TimeEntry {
  id: number;
  project: IdName;
  issue: { id: number } | null;
  user: IdName;
  activity: IdName;
  hours: number;
  comments: string | null;
  spent_on: string;
}

export type DayStatus =
  | "weekend"
  | "holiday"
  | "leave"
  | "full"
  | "over"
  | "partial"
  | "missing"
  | "future";

export interface DaySummary {
  date: string;
  status: DayStatus;
  hours: number;
  leave_hours: number;
  entries: TimeEntry[];
}

export interface MonthSummary {
  year: number;
  month: number;
  days: DaySummary[];
  total_hours: number;
  expected_hours: number;
  missing_hours: number;
  missing_days: number;
}

export interface ToolStatus {
  name: string;
  version: string | null;
  install_hint: string;
}

// Review draft and execution (crates/logtime-engine/src/model.rs).

export type Target =
  | { type: "issue"; issue_id: number; label: string }
  | { type: "new_subtask"; key: string };

export interface PlanItem {
  id: string;
  date: string;
  target: Target;
  hours: number;
  activity_id: number | null;
  comments: string;
  kind: "work" | "leave";
  last_error: string | null;
}

export interface SubtaskDraft {
  key: string;
  parent_id: number;
  parent_subject: string;
  project_id: number;
  project_name: string;
  tracker_id: number;
  subject: string;
}

export interface Draft {
  subtasks: SubtaskDraft[];
  items: PlanItem[];
}

export interface DayReview {
  date: string;
  is_workday: boolean;
  existing_hours: number;
  planned_hours: number;
  total_hours: number;
  existing: TimeEntry[];
  items: PlanItem[];
  warnings: string[];
}

export interface ReviewView {
  days: DayReview[];
  subtasks: SubtaskDraft[];
  hours_per_day: number;
}

export interface ApiCall {
  method: string;
  path: string;
  body: unknown;
}

export type OpPayload =
  | ({ op: "create_subtask" } & SubtaskDraft)
  | ({ op: "create_time_entry" } & PlanItem);

export type OpStatus = "pending" | "running" | "done" | "failed" | "undone";

export interface Op {
  id: number;
  seq: number;
  payload: OpPayload;
  status: OpStatus;
  result_id: number | null;
  error: string | null;
}

export interface Batch {
  id: number;
  created_at: string;
  label: string;
  redmine_url: string;
  ops: Op[];
}

export interface ProgressEvent {
  batch_id: number;
  op: Op;
}

// AI (crates/ai-bridge, src-tauri/src/ai_commands.rs).

export interface ParentRule {
  tracker_ids: number[];
  tracker_names: string[];
  author_ids: number[];
  author_names: string[];
  role_names: string[];
  description: string;
}

export interface Profile {
  parent_rule: ParentRule;
  subtask_tracker_id: number | null;
  leave_issue_id: number | null;
  projects: { id: number; name: string; identifier: string }[];
  naming: string;
  summary: string;
  user_notes: string;
  generated_at: string | null;
}

export interface ReportTask {
  key: string;
  text: string;
  project_hint: string;
  parent_issue_id: number | null;
  parent_confidence: number;
  parent_reason: string;
  existing_subtask_id: number | null;
  subtask_subject: string;
  comment: string;
  days: { date: string; weight: number }[];
}

export interface IssueBrief {
  id: number;
  subject: string;
  project_id: number;
  project_name: string;
  tracker: string;
  status: string;
  parent_id: number | null;
}

export interface Duplicate {
  date: string;
  reason: string;
}

export interface TaskView {
  task: ReportTask;
  parent: IssueBrief | null;
  existing_subtask: IssueBrief | null;
  problems: string[];
  /** Days dropped because the work is already logged or queued. */
  duplicates: Duplicate[];
}

export interface DayCapacity {
  date: string;
  workday: boolean;
  existing_hours: number;
  queued_hours: number;
  leave_hours: number;
  capacity: number;
}

export interface ReportView {
  analysis: {
    period_from: string;
    period_to: string;
    leave: { date: string; hours: number }[];
    questions: string[];
    tasks: ReportTask[];
  };
  tasks: TaskView[];
  days: DayCapacity[];
  skipped_future: string[];
  /** Set when this is one calendar day's report. */
  day: string | null;
}

export interface DayPlan {
  date: string;
  capacity: number;
  existing_hours: number;
  queued_hours: number;
  leave_hours: number;
  allocations: { task: string; date: string; hours: number }[];
  warnings: string[];
}

export interface AiProgress {
  text: string;
}

export function issueBrief(i: Issue): IssueBrief {
  return {
    id: i.id,
    subject: i.subject,
    project_id: i.project.id,
    project_name: i.project.name,
    tracker: i.tracker.name,
    status: i.status.name,
    parent_id: i.parent?.id ?? null,
  };
}

// Tauri maps camelCase invoke keys onto snake_case Rust parameters.
export const api = {
  getSettings: () => invoke<SettingsView>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  saveApiKey: (apiKey: string) => invoke<void>("save_api_key", { apiKey }),
  testConnection: (redmineUrl: string, apiKey: string | null, acceptInvalidCerts: boolean) =>
    invoke<User>("test_connection", { redmineUrl, apiKey, acceptInvalidCerts }),
  listActivities: () => invoke<Activity[]>("list_activities"),
  listTrackers: () => invoke<Tracker[]>("list_trackers"),
  lookupIssue: (id: number) => invoke<Issue>("lookup_issue", { id }),
  monthSummary: (year: number, month: number) =>
    invoke<MonthSummary>("month_summary", { year, month }),
  checkTools: () => invoke<ToolStatus[]>("check_tools"),

  getDraft: () => invoke<Draft>("get_draft"),
  saveDraft: (draft: Draft) => invoke<void>("save_draft", { draft }),
  reviewDraft: () => invoke<ReviewView>("review_draft"),
  previewItems: (itemIds: string[]) => invoke<ApiCall[]>("preview_items", { itemIds }),
  executeItems: (itemIds: string[], label: string) =>
    invoke<Batch>("execute_items", { itemIds, label }),
  resumeBatch: (batchId: number) => invoke<Batch>("resume_batch", { batchId }),
  undoBatch: (batchId: number) => invoke<Batch>("undo_batch", { batchId }),
  listBatches: () => invoke<Batch[]>("list_batches"),
  onProgress: (cb: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("batch-progress", (e) => cb(e.payload)),

  aiTest: () => invoke<string>("ai_test"),
  aiCancel: () => invoke<void>("ai_cancel"),
  getProfile: () => invoke<Profile | null>("get_profile"),
  saveProfile: (profile: Profile) => invoke<void>("save_profile", { profile }),
  discoverProfile: () => invoke<Profile>("discover_profile"),
  getLastReport: (day: string | null) =>
    invoke<{ text: string; day: string | null; view: ReportView | null }>("get_last_report", { day }),
  analyzeReport: (text: string, day: string | null) => invoke<ReportView>("analyze_report", { text, day }),
  planReport: (view: ReportView, excluded: string[]) => invoke<DayPlan[]>("plan_report", { view, excluded }),
  onAiProgress: (cb: (p: AiProgress) => void): Promise<UnlistenFn> =>
    listen<AiProgress>("ai-progress", (e) => cb(e.payload)),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function newId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
