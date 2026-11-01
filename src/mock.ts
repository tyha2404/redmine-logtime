// Canned backend for browser-only design work (VITE_MOCK=1). Mirrors the
// shapes of the Tauri commands with an October 2026 that has every day state.
import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import type {
  Batch,
  DayPlan,
  DayStatus,
  DaySummary,
  Issue,
  LearnedTask,
  Op,
  OpPayload,
  Profile,
  ReportView,
  Settings,
  TimeEntry,
  WritePlan,
} from "./api";

const TODAY = "2026-10-29";
const HPD = 8;

let settings: Settings = {
  redmine_url: "https://redmine.congty.vn",
  accept_invalid_certs: false,
  hours_per_day: HPD,
  workdays: [1, 2, 3, 4, 5],
  holidays: ["2026-09-02"],
  leave_types: [
    { key: "annual", label: "Nghỉ phép năm", project_id: 108, project_name: "Annual Leave", activity_id: 21, verb: "xin nghỉ phép", days_per_year: 12 },
    { key: "sick", label: "Nghỉ đột xuất", project_id: 109, project_name: "Sick Leave", activity_id: 22, verb: "xin nghỉ đột xuất", days_per_year: 4 },
    { key: "unpaid", label: "Nghỉ không lương", project_id: 110, project_name: "Unpaid Leave", activity_id: 23, verb: "xin nghỉ không lương", days_per_year: null },
  ],
  leave_name: "BE - Tên",
  default_activity_id: 9,
  subtask_tracker_id: 4,
  claude_ignore_user_settings: true,
  reminder_enabled: true,
  reminder_time: "17:30",
  start_at_login: false,
};

const iso = (d: number) => `2026-10-${String(d).padStart(2, "0")}`;
let nextEntry = 1000;
const entry = (day: number, issue: number, hours: number, comments: string, project = "Dự án ABC"): TimeEntry => ({
  id: nextEntry++,
  project: { id: project === "Annual Leave" ? 108 : 1, name: project },
  issue: { id: issue },
  user: { id: 6, name: "Dev A" },
  activity: { id: 9, name: "Development" },
  hours,
  comments,
  spent_on: iso(day),
});

const ENTRIES: TimeEntry[] = [
  ...[1, 2, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16].flatMap((d) => [
    entry(d, 4, 5, "Màn hình danh sách công nợ"),
    entry(d, 5, 3, "Review Module Sự kiện"),
  ]),
  entry(19, 4, 8, "Màn hình danh sách công nợ"),
  entry(20, 4, 3, "Filter + paging"),
  entry(20, 5, 2, "Review PR"),
  entry(21, 16930, 8, "", "Annual Leave"),
  entry(23, 4, 6, "Fix bug"),
  entry(23, 5, 3, "Review"),
  entry(26, 4, 5, "Sửa feedback công nợ"),
  entry(26, 5, 3, "Review Module Sự kiện"),
  entry(27, 4, 4, "Sửa feedback công nợ"),
];

const SUB_1 = {
  key: "sub-1",
  parent_id: 3,
  parent_subject: "Báo cáo doanh số theo vùng",
  project_id: 1,
  project_name: "Dự án ABC",
  tracker_id: 4,
  subject: "[FE] Biểu đồ doanh số theo vùng",
};

function item(
  id: string,
  day: number,
  target: WritePlan["items"][number]["target"],
  hours: number,
  comments: string,
  kind: "work" | "leave" = "work",
) {
  return { id, date: iso(day), target, hours, activity_id: 9, comments, kind };
}

function isWorkday(date: string) {
  const d = new Date(date + "T00:00:00");
  const wd = ((d.getDay() + 6) % 7) + 1;
  return settings.workdays.includes(wd) && !settings.holidays.includes(date);
}

function monthSummary(year: number, month: number) {
  const last = new Date(year, month, 0).getDate();
  const days: DaySummary[] = [];
  let total = 0,
    expected = 0,
    missingHours = 0,
    missingDays = 0;
  for (let d = 1; d <= last; d++) {
    const date = `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
    const entries = ENTRIES.filter((e) => e.spent_on === date);
    const hours = entries.reduce((s, e) => s + e.hours, 0);
    const leave = entries
      .filter((e) => settings.leave_types.some((t) => t.project_id === e.project.id))
      .reduce((s, e) => s + e.hours, 0);
    total += hours;
    let status: DayStatus;
    if (settings.holidays.includes(date)) status = "holiday";
    else if (!isWorkday(date)) status = "weekend";
    else {
      expected += HPD;
      status =
        leave >= HPD ? "leave" : hours > HPD ? "over" : hours >= HPD ? "full" : hours > 0 ? "partial" : date > TODAY ? "future" : "missing";
      if (date <= TODAY && hours < HPD) {
        missingHours += HPD - hours;
        missingDays++;
      }
    }
    days.push({ date, status, hours, leave_hours: leave, entries });
  }
  return { year, month, days, total_hours: total, expected_hours: expected, missing_hours: missingHours, missing_days: missingDays };
}

const PROFILE: Profile = {
  parent_rule: {
    tracker_ids: [2],
    tracker_names: ["Feature"],
    author_ids: [5],
    author_names: ["BA B"],
    role_names: ["Manager"],
    description:
      "Task cha là issue tracker Feature (không có parent) do BA B tạo; developer tạo sub-task tracker Task dưới Feature đó.",
  },
  subtask_tracker_id: 4,
  leave_issue_id: 6,
  projects: [
    { id: 1, name: "Dự án ABC", identifier: "du-an-abc" },
    { id: 2, name: "Nội bộ", identifier: "noi-bo" },
  ],
  naming: "Sub-task đặt tên tiếng Việt, đôi khi có tiền tố tầng như [FE].",
  summary:
    "Developer là Dev A (id 6, role Developer), làm việc trong 2 dự án: Dự án ABC và Nội bộ. Task cha là các issue tracker Feature không có parent, đều do BA B (id 5) tạo: #1 “Quản lý công nợ khách hàng”, #2 “Module Sự kiện”, #3 “Báo cáo doanh số theo vùng”. Developer tự tạo sub-task tracker Task, gán cho chính mình và log time trên sub-task. Để tìm task cha cho một dòng báo cáo, so khớp nội dung với subject của Feature; nếu đã có sub-task của developer dưới Feature đó thì dùng lại. Nghỉ phép: issue con dưới task cha HR tạo mỗi tháng.",
  user_notes: "",
  generated_at: "2026-10-28T16:40:00Z",
};

const REPORT_TEXT = `- Lọc theo vùng cho biểu đồ doanh số
- Review Module Sự kiện
- Sửa feedback công nợ`;

const brief = (id: number, subject: string, tracker: string, parent: number | null = null) => ({
  id,
  subject,
  project_id: 1,
  project_name: "Dự án ABC",
  tracker,
  status: "New",
  parent_id: parent,
});

const task = (
  key: string,
  text: string,
  parent: number,
  confidence: number,
  reason: string,
  existing: number | null,
  days: string[],
) => ({
  key,
  text,
  project_hint: "Dự án ABC",
  parent_issue_id: parent,
  parent_confidence: confidence,
  parent_reason: reason,
  existing_subtask_id: existing,
  subtask_subject: text,
  comment: text,
  days: days.map((date) => ({ date, weight: 1 })),
});

// A one-day report for "today": one line duplicates work already logged.
const REPORT_VIEW: ReportView = {
  analysis: { period_from: TODAY, period_to: TODAY, leave: [], questions: [], tasks: [] },
  tasks: [
    {
      task: task("bieu-do", "Lọc theo vùng cho biểu đồ doanh số", 3, 0.88, "Khớp Feature #3 “Báo cáo doanh số theo vùng”.", null, []),
      parent: brief(3, "Báo cáo doanh số theo vùng", "Feature"),
      existing_subtask: null,
      problems: [],
      duplicates: [{ date: TODAY, reason: "Đã log 5h vào #12 trên Redmine." }],
    },
    {
      task: task("su-kien", "Review Module Sự kiện", 2, 0.97, "“Module Sự kiện” khớp chính xác Feature #2.", 5, [TODAY]),
      parent: brief(2, "Module Sự kiện", "Feature"),
      existing_subtask: brief(5, "Review Module Sự kiện", "Task", 2),
      problems: [],
      duplicates: [],
    },
    {
      task: task("cong-no", "Sửa feedback công nợ", 1, 0.55, "Có thể thuộc Feature #1 “Quản lý công nợ khách hàng”.", 4, [TODAY]),
      parent: brief(1, "Quản lý công nợ khách hàng", "Feature"),
      existing_subtask: brief(4, "[FE] Màn hình danh sách công nợ", "Task", 1),
      problems: ["AI không chắc chắn về task cha này, hãy kiểm tra."],
      duplicates: [],
    },
  ],
  days: [{ date: TODAY, workday: true, existing_hours: 0, leave_hours: 0, capacity: 8 }],
  skipped_future: [],
  day: TODAY,
};
REPORT_VIEW.analysis.tasks = REPORT_VIEW.tasks.map((t) => t.task);

// A weekly report for the "Ghi logtime" page: several days, a half day off
// and one task without a parent.
const WEEK_VIEW: ReportView = {
  analysis: {
    period_from: iso(27),
    period_to: iso(29),
    leave: [{ date: iso(28), hours: 4, kind: "annual" }],
    questions: [],
    tasks: [],
  },
  tasks: [
    {
      task: task("cong-no", "Sửa feedback công nợ", 1, 0.93, "Khớp Feature #1 “Quản lý công nợ khách hàng”.", 4, [iso(27), iso(28)]),
      parent: brief(1, "Quản lý công nợ khách hàng", "Feature"),
      existing_subtask: brief(4, "[FE] Màn hình danh sách công nợ", "Task", 1),
      problems: [],
      duplicates: [],
    },
    {
      task: task("bieu-do", "Lọc theo vùng cho biểu đồ doanh số", 3, 0.88, "Khớp Feature #3 “Báo cáo doanh số theo vùng”.", null, [iso(28), iso(29)]),
      parent: brief(3, "Báo cáo doanh số theo vùng", "Feature"),
      existing_subtask: null,
      problems: [],
      duplicates: [],
    },
    {
      task: task("su-kien", "Review Module Sự kiện", 2, 0.97, "“Module Sự kiện” khớp chính xác Feature #2.", 5, [iso(29)]),
      parent: brief(2, "Module Sự kiện", "Feature"),
      existing_subtask: brief(5, "Review Module Sự kiện", "Task", 2),
      problems: [],
      duplicates: [{ date: iso(27), reason: "Đã log 3h vào #5 trên Redmine." }],
    },
    {
      task: { ...task("hop", "Họp sprint planning", 0, 0, "", null, [iso(29)]), parent_issue_id: null },
      parent: null,
      existing_subtask: null,
      problems: ["Không tìm thấy task cha phù hợp, hãy chọn."],
      duplicates: [],
    },
  ],
  days: [
    { date: iso(27), workday: true, existing_hours: 4, leave_hours: 0, capacity: 4 },
    { date: iso(28), workday: true, existing_hours: 0, leave_hours: 4, capacity: 4 },
    { date: iso(29), workday: true, existing_hours: 0, leave_hours: 0, capacity: 8 },
  ],
  skipped_future: [],
  day: null,
};
WEEK_VIEW.analysis.tasks = WEEK_VIEW.tasks.map((t) => t.task);

const issue = (id: number, subject: string, tracker: [number, string], parent: number | null = null, mine = false): Issue => ({
  id,
  project: { id: 1, name: "Dự án ABC" },
  tracker: { id: tracker[0], name: tracker[1] },
  status: { id: 1, name: id === 7 ? "Closed" : "New" },
  subject,
  parent: parent ? { id: parent } : null,
  author: { id: 5, name: "BA B" },
  assigned_to: mine ? { id: 6, name: "Dev A" } : null,
});

const ISSUES: Issue[] = [
  issue(1, "Quản lý công nợ khách hàng", [2, "Feature"]),
  issue(2, "Module Sự kiện", [2, "Feature"]),
  issue(3, "Báo cáo doanh số theo vùng", [2, "Feature"]),
  issue(8, "Họp và nghiệp vụ nội bộ tháng 10", [3, "Support"]),
  issue(9, "Sprint 42 – họp và review", [3, "Support"]),
  issue(4, "[FE] Màn hình danh sách công nợ", [4, "Task"], 1, true),
  issue(7, "[BE] API công nợ", [4, "Task"], 1, true),
  issue(5, "Review Module Sự kiện", [4, "Task"], 2, true),
];

// Writes: progress events like the real backend; ?writefail=1 makes the
// last time entry of each write fail once.
const listeners = new Map<string, Set<EventCallback<unknown>>>();
const emit = (event: string, payload: unknown) =>
  listeners.get(event)?.forEach((cb) => cb({ event, id: 0, payload } as Parameters<EventCallback<unknown>>[0]));
const failOnce = new URLSearchParams(location.search).has("writefail");
let nextOp = 1000;
let nextBatch = 10;
let nextIssue = 20000;
const created = new Map<string, number>();

async function runOps(batch: Batch, pick: (op: Op) => boolean, fail: (op: Op) => string | null) {
  for (const op of batch.ops) {
    if (!pick(op)) continue;
    await delay(null, 350);
    const why = fail(op);
    if (why) {
      Object.assign(op, { status: "failed", error: why });
    } else if (op.payload.op === "create_subtask") {
      const id = created.get(op.payload.key) ?? nextIssue++;
      created.set(op.payload.key, id);
      Object.assign(op, { status: "done", result_id: id, error: null });
    } else {
      const p = op.payload;
      const issueId = p.target.type === "issue" ? p.target.issue_id : created.get(p.target.key) ?? 0;
      const e = entry(Number(p.date.slice(8)), issueId, p.hours, p.comments);
      ENTRIES.push(e);
      Object.assign(op, { status: "done", result_id: e.id, error: null });
    }
    emit("batch-progress", { batch_id: batch.id, op: { ...op } });
  }
  return structuredClone(batch);
}

function planReport(view: ReportView, excluded: string[]): DayPlan[] {
  return view.days.map((d) => {
    const demands = view.analysis.tasks
      .filter((t) => !excluded.includes(t.key))
      .flatMap((t) => t.days.filter((x) => x.date === d.date).map((x) => ({ task: t.key, w: x.weight })));
    const sum = demands.reduce((s, x) => s + x.w, 0);
    let left = Math.floor(d.capacity * 2);
    const allocations = demands.map((x, i) => {
      const steps = i === demands.length - 1 ? left : Math.floor((d.capacity * 2 * x.w) / sum);
      left -= steps;
      return { task: x.task, date: d.date, hours: steps / 2 };
    });
    const warnings = d.capacity > 0 && demands.length === 0 ? [`Không có việc nào trong báo cáo cho ngày này, còn trống ${d.capacity}h.`] : [];
    return { ...d, allocations: allocations.filter((a) => a.hours > 0), warnings };
  });
}

const BATCHES: Batch[] = [
  {
    id: 2,
    created_at: "2026-10-28T09:12:00Z",
    label: "3 logtime, 22/10–23/10",
    redmine_url: settings.redmine_url,
    ops: [
      { id: 21, seq: 0, payload: { op: "create_subtask", ...SUB_1, subject: "[FE] Export Excel công nợ" }, status: "done", result_id: 12, error: null },
      { id: 22, seq: 1, payload: { op: "create_time_entry", ...item("x1", 22, { type: "new_subtask", key: "sub-1" }, 5, "Export Excel") }, status: "done", result_id: 1201, error: null },
      { id: 23, seq: 2, payload: { op: "create_time_entry", ...item("x2", 22, { type: "issue", issue_id: 5, label: "#5 Review Module Sự kiện" }, 3, "Review") }, status: "done", result_id: 1202, error: null },
      { id: 24, seq: 3, payload: { op: "create_time_entry", ...item("x3", 29, { type: "new_subtask", key: "sub-1" }, 5, "Lọc theo vùng") }, status: "failed", result_id: null, error: "Redmine từ chối dữ liệu: Activity cannot be blank" },
    ],
  },
  {
    id: 1,
    created_at: "2026-10-20T17:45:00Z",
    label: "1 logtime, 21/10",
    redmine_url: settings.redmine_url,
    ops: [
      { id: 11, seq: 0, payload: { op: "create_time_entry", ...item("y1", 21, { type: "issue", issue_id: 16930, label: "#16930 BE - Tên xin nghỉ phép ngày 21/10/2026" }, 8, "Nghỉ phép năm", "leave") }, status: "undone", result_id: 1100, error: null },
    ],
  },
];

let fresh = new URLSearchParams(location.search).has("setup");

// ?gchat=0 starts with Google Chat not connected.
let gchat = new URLSearchParams(location.search).get("gchat") === "0"
  ? { connected: false, email: null as string | null, client_id: null as string | null }
  : { connected: true, email: "dev.a@congty.vn", client_id: "123-abc.apps.googleusercontent.com" };

/** Tasks remembered from earlier writes; `?memory=0` starts empty. */
let memory: LearnedTask[] =
  new URLSearchParams(location.search).get("memory") === "0"
    ? []
    : [
        { text: "Sửa feedback công nợ", parent_id: 1, parent_subject: "Quản lý công nợ khách hàng", subtask_id: 4, subtask_subject: "[FE] Màn hình danh sách công nợ", project_hint: "ABC", comment: "" },
        { text: "Review Module Sự kiện", parent_id: 2, parent_subject: "Module Sự kiện", subtask_id: 5, subtask_subject: "Review Module Sự kiện", project_hint: "ABC", comment: "" },
      ];

const CHAT_MESSAGES = [
  { name: "spaces/TEAM/messages/1", create_time: "2026-10-26T10:05:00Z", text: "Thứ 2 26/10:\n- Sửa feedback công nợ\n- Review Module Sự kiện" },
  { name: "spaces/TEAM/messages/2", create_time: "2026-10-27T10:12:00Z", text: "Báo cáo hôm nay:\n- Sửa feedback công nợ (filter + export)" },
  { name: "spaces/TEAM/messages/3", create_time: "2026-10-27T10:20:00Z", text: "Mai em xin nghỉ buổi sáng ạ" },
  { name: "spaces/TEAM/messages/4", create_time: "2026-10-28T10:01:00Z", text: "Báo cáo hôm nay:\n- [FE] Biểu đồ doanh số theo vùng\n- Review PR module Sự kiện" },
];

/** `?slow=1` stretches every answer, to look at loading states (skeletons). */
const SLOW = new URLSearchParams(location.search).has("slow") ? 15 : 1;
const delay = <T,>(v: T, ms = 120) => new Promise<T>((r) => setTimeout(() => r(v), ms * SLOW));

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "get_settings":
      // ?setup=1 shows the first-run screen of a fresh install.
      if (fresh) return delay({ settings: { ...settings, redmine_url: "" }, has_api_key: false, secret_storage: "keychain" });
      return delay({ settings, has_api_key: true, secret_storage: "keychain" });
    case "save_api_key":
      fresh = false;
      return delay(undefined);
    case "test_connection":
      return delay({ id: 6, login: "dev.a", firstname: "Dev", lastname: "A" }, 900);
    case "save_settings":
      settings = args.settings as Settings;
      return delay(undefined);
    case "month_summary":
      return delay(monthSummary(args.year as number, args.month as number), 250);
    case "list_batches":
      return delay(structuredClone(BATCHES));
    case "list_activities":
      return delay([
        { id: 8, name: "Design", is_default: false, active: true },
        { id: 9, name: "Development", is_default: true, active: true },
        { id: 21, name: "Annual Leave", is_default: false, active: true },
        { id: 22, name: "Sick Leave", is_default: false, active: true },
        { id: 23, name: "Unpaid Leave", is_default: false, active: true },
      ]);
    case "list_trackers":
      return delay([
        { id: 1, name: "Bug" },
        { id: 2, name: "Feature" },
        { id: 3, name: "Support" },
        { id: 4, name: "Task" },
      ]);
    case "check_tools":
      return delay([
        { name: "claude", version: "2.1.312 (Claude Code)", install_hint: "" },
        { name: "uvx", version: "uv-tool-uvx 0.13.6", install_hint: "" },
      ]);
    case "get_profile":
      return delay(PROFILE);
    case "get_last_report":
      return delay(
        args.day === TODAY ? { text: REPORT_TEXT, day: TODAY, view: REPORT_VIEW } : { text: "", day: args.day ?? null, view: null },
      );
    case "install_update":
      for (const downloaded of [2, 5, 8, 10]) {
        await delay(undefined, 250);
        emit("update-progress", { downloaded: downloaded * 1_000_000, total: 10_000_000 });
      }
      throw "Bản mock không cài được; bản thật sẽ tự khởi động lại.";
    case "open_log_dir":
      return delay(undefined, 50);
    case "quick_report": {
      if (memory.length === 0) throw "Chưa có việc nào được ghi nhớ. Phân tích bằng AI và ghi lần đầu, app sẽ nhớ cho các lần sau.";
      const known = (t: string) => memory.some((m) => m.text.toLowerCase() === t.toLowerCase());
      const v = structuredClone(WEEK_VIEW);
      v.tasks = v.tasks
        .filter((t) => known(t.task.text))
        .map((t) => ({ ...t, problems: [], task: { ...t.task, parent_confidence: 1, parent_reason: "Lần trước bạn đã chọn task này." } }));
      v.analysis.tasks = v.tasks.map((t) => t.task);
      v.analysis.leave = [];
      v.days = v.days.map((d) => ({ ...d, leave_hours: 0, capacity: Math.max(HPD - d.existing_hours, 0) }));
      const unknown = WEEK_VIEW.tasks.filter((t) => !known(t.task.text)).map((t) => `“${t.task.text}”`);
      v.analysis.questions = [
        ...(unknown.length ? [`Chưa nhận ra ${unknown.length} dòng (chưa ghi lần nào): ${unknown.join(", ")}. Dùng Phân tích bằng AI cho các việc mới.`] : []),
        "Khớp nhanh không ghi nghỉ phép: “T4 28/10 nghỉ buổi sáng”. Dùng Xin nghỉ trên Lịch tháng.",
      ];
      if (args.day) {
        // A calendar day's report: every matched line is that day's work.
        const day = args.day as string;
        v.tasks = v.tasks.map((t) => ({ ...t, task: { ...t.task, days: [{ date: day, weight: 1 }] } }));
        v.analysis = { ...v.analysis, period_from: day, period_to: day, tasks: v.tasks.map((t) => t.task) };
        v.days = [{ date: day, workday: true, existing_hours: 0, leave_hours: 0, capacity: HPD }];
        v.day = day;
      }
      return delay(v, 300);
    }
    case "remember_tasks": {
      for (const t of args.tasks as LearnedTask[]) {
        const i = memory.findIndex((m) => m.text.toLowerCase() === t.text.toLowerCase());
        if (i >= 0) memory[i] = t;
        else memory.push(t);
      }
      return delay(memory.length, 50);
    }
    case "memory_count":
      return delay(memory.length, 50);
    case "forget_memory":
      memory = [];
      return delay(undefined, 50);
    case "analyze_report":
      return delay(args.day ? { ...REPORT_VIEW, day: args.day as string } : structuredClone(WEEK_VIEW), 1200);
    case "discover_leave":
      return delay({ types: settings.leave_types, name: "BE - Tên" }, 700);
    case "leave_parent": {
      const [y, m] = String(args.date).split("-");
      const t = settings.leave_types.find((x) => x.project_id === args.projectId);
      if (m === "11") throw `Chưa có task cha nghỉ phép tháng ${m}/${y} trong ${t?.project_name}. HR thường tạo vào đầu tháng; khi có rồi thử lại.`;
      const names: Record<string, [number, string]> = {
        annual: [15973, "Xin nghỉ phép Tháng"],
        sick: [15974, "Xin nghỉ phép Đột Xuất Tháng"],
        unpaid: [16198, "Xin nghỉ phép KL Tháng"],
      };
      const [id, name] = names[t?.key ?? "annual"];
      return delay({ id, subject: `${name} ${m}.${y}`, project_id: t?.project_id, project_name: t?.project_name, tracker_id: 5 }, 300);
    }
    case "leave_entries": {
      if (args.year !== 2026) return delay([]);
      const leave = (date: string, projectId: number, project: string, hours: number, issue: number): TimeEntry => ({
        id: nextEntry++,
        project: { id: projectId, name: project },
        issue: { id: issue },
        user: { id: 6, name: "Dev A" },
        activity: { id: 21, name: project },
        hours,
        comments: "",
        spent_on: date,
      });
      return delay(
        [
          leave("2026-01-02", 108, "Annual Leave", 8, 3335),
          leave("2026-02-12", 108, "Annual Leave", 8, 6105),
          leave("2026-02-13", 108, "Annual Leave", 8, 6106),
          leave("2026-03-19", 108, "Annual Leave", 7, 7657),
          leave("2026-05-27", 108, "Annual Leave", 8, 10296),
          leave("2026-05-28", 108, "Annual Leave", 8, 10297),
          leave("2026-07-27", 108, "Annual Leave", 4, 12667),
          leave("2026-08-25", 108, "Annual Leave", 8, 13930),
          leave("2026-10-21", 108, "Annual Leave", 8, 16930),
          leave("2026-10-19", 109, "Sick Leave", 8, 16659),
        ],
        400,
      );
    }
    case "clear_last_report":
      return delay(undefined);
    case "plan_report":
      return delay(planReport(args.view as ReportView, args.excluded as string[]));
    case "lookup_issue":
      return delay({
        id: args.id,
        project: { id: 1, name: "Dự án ABC" },
        tracker: { id: 2, name: "Feature" },
        status: { id: 1, name: "New" },
        subject: "Quản lý công nợ khách hàng",
        parent: null,
        author: { id: 5, name: "BA B" },
        assigned_to: null,
      });
    case "search_issues": {
      const q = String(args.query).trim().toLowerCase().replace(/^#/, "");
      const found = /^\d+$/.test(q) ? ISSUES.filter((i) => String(i.id) === q) : ISSUES.filter((i) => i.subject.toLowerCase().includes(q));
      return delay(found.filter((i) => i.status.name !== "Closed" || /^\d+$/.test(q)), 350);
    }
    case "list_children":
      return delay(ISSUES.filter((i) => i.parent?.id === args.parentId && i.assigned_to), 300);
    case "preview_plan": {
      const plan = args.plan as WritePlan;
      return delay([
        ...plan.subtasks.map((s) => ({ method: "POST", path: "/issues.json", body: { issue: { subject: s.subject, parent_issue_id: s.parent_id } } })),
        ...plan.items.map((i) => ({ method: "POST", path: "/time_entries.json", body: { time_entry: { spent_on: i.date, hours: i.hours, comments: i.comments } } })),
      ]);
    }
    case "write_plan": {
      const plan = args.plan as WritePlan;
      const used = plan.subtasks.filter((s) => plan.items.some((i) => i.target.type === "new_subtask" && i.target.key === s.key));
      const payloads: OpPayload[] = [
        ...used.map((s) => ({ op: "create_subtask" as const, ...s })),
        ...[...plan.items].sort((a, b) => a.date.localeCompare(b.date)).map((i) => ({ op: "create_time_entry" as const, ...i })),
      ];
      const batch: Batch = {
        id: nextBatch++,
        created_at: new Date().toISOString(),
        label: String(args.label),
        redmine_url: settings.redmine_url,
        ops: payloads.map((payload, seq) => ({ id: nextOp++, seq, payload, status: "pending", result_id: null, error: null })),
      };
      BATCHES.unshift(batch);
      const last = batch.ops.at(-1)!.id;
      return runOps(batch, () => true, (op) => (failOnce && op.id === last ? "Redmine từ chối dữ liệu: Issue is closed" : null));
    }
    case "retry_batch": {
      const batch = BATCHES.find((b) => b.id === args.batchId)!;
      return runOps(batch, (op) => op.status === "failed", () => null);
    }
    case "resume_batch": {
      const batch = BATCHES.find((b) => b.id === args.batchId)!;
      return runOps(batch, (op) => op.status === "pending" || op.status === "running", () => null);
    }
    case "undo_batch": {
      const batch = BATCHES.find((b) => b.id === args.batchId)!;
      for (const op of [...batch.ops].reverse()) {
        if (op.payload.op !== "create_time_entry" || op.status !== "done") continue;
        await delay(null, 250);
        const at = ENTRIES.findIndex((e) => e.id === op.result_id);
        if (at >= 0) ENTRIES.splice(at, 1);
        op.status = "undone";
        emit("batch-progress", { batch_id: batch.id, op: { ...op } });
      }
      return structuredClone(batch);
    }
    case "open_issue":
      window.open(`${settings.redmine_url}/issues/${args.id}`, "_blank", "noopener");
      return delay(undefined);
    case "gchat_status":
      return delay(gchat);
    case "gchat_connect":
      gchat = { connected: true, email: "dev.a@congty.vn", client_id: String(args.clientId) };
      return delay(gchat, 1500);
    case "gchat_cancel_connect":
      return delay(undefined);
    case "gchat_disconnect":
      gchat = { connected: false, email: null, client_id: gchat.client_id };
      return delay(gchat);
    case "gchat_spaces":
      if (!gchat.connected) throw "Chưa kết nối Google Chat. Vào Cài đặt → Google Chat để kết nối.";
      return delay(
        [
          { name: "spaces/OTHER", display_name: "Dự án XYZ" },
          { name: "spaces/TEAM", display_name: "Team ABC - Báo cáo hằng ngày" },
        ],
        400,
      );
    case "gchat_messages": {
      if (args.space !== "spaces/TEAM") return delay([], 500);
      const from = new RealDate(`${args.from}T00:00:00`).getTime();
      const to = new RealDate(`${args.to}T23:59:59`).getTime();
      return delay(
        CHAT_MESSAGES.filter((m) => {
          const t = new RealDate(m.create_time).getTime();
          return t >= from && t <= to;
        }),
        700,
      );
    }
    case "ai_test":
      return delay("Sẵn sàng phân tích báo cáo.", 800);
    default:
      throw `Mock chưa hỗ trợ lệnh ${cmd}`;
  }
}

export async function mockListen(event: string, cb: EventCallback<unknown>): Promise<UnlistenFn> {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event)!.add(cb);
  return () => listeners.get(event)?.delete(cb);
}

// Pin "today" so screenshots are stable.
const RealDate = Date;
class FixedDate extends RealDate {
  constructor(...args: ConstructorParameters<typeof Date> | []) {
    if (args.length === 0) super(`${TODAY}T10:00:00`);
    else super(...(args as ConstructorParameters<typeof Date>));
  }
  static now() {
    return new RealDate(`${TODAY}T10:00:00`).getTime();
  }
}
globalThis.Date = FixedDate as DateConstructor;
