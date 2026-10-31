# Seeds a local Redmine that mimics the real setup: BA-created parent issues,
# developer sub-tasks, a shared leave issue, and some October 2026 time entries.
# Idempotent: re-running only prints the developer API key.

Setting.rest_api_enabled = '1'
Redmine::DefaultData::Loader.load('en') unless Tracker.exists?

def make_user(login, first, last)
  User.find_by_login(login) || User.new(firstname: first, lastname: last, mail: "#{login}@example.test").tap do |u|
    u.login = login
    u.password = u.password_confirmation = 'password123'
    u.status = User::STATUS_ACTIVE
    u.save!
  end
end

ba  = make_user('ba1', 'Lan', 'Nguyễn')
dev = make_user('dev1', 'Minh', 'Trần')

# Lets developers undo their own entries (default Developer role cannot).
Role.find_by(name: 'Developer')&.add_permission!(:edit_own_time_entries)

if Project.exists?(identifier: 'davipharm-salex')
  puts "Already seeded. dev1 API key: #{dev.api_key}"
  exit
end

open_status = IssueStatus.order(:position).first
priority = IssuePriority.default || IssuePriority.first

task = Tracker.find_by(name: 'Task') || Tracker.create!(name: 'Task', default_status: open_status)
WorkflowRule.copy(Tracker.find_by(name: 'Bug'), nil, [task], nil) if WorkflowRule.where(tracker_id: task.id).none?

def make_project(name, identifier, members)
  Project.create!(name: name, identifier: identifier, is_public: false).tap do |p|
    p.trackers = Tracker.all
    p.enabled_module_names = %w[issue_tracking time_tracking]
    p.save!
    members.each do |user, role_name|
      Member.new(project: p, user: user, role_ids: [Role.find_by!(name: role_name).id]).save!
    end
  end
end

salex = make_project('Davipharm - Salex', 'davipharm-salex', [[ba, 'Manager'], [dev, 'Developer']])
internal = make_project('Nội bộ', 'noi-bo', [[dev, 'Developer']])

def make_issue(project, tracker, subject, author, assignee: nil, parent: nil)
  Issue.new(project: project, tracker: tracker, subject: subject, author: author,
            status: tracker.default_status, priority: IssuePriority.default || IssuePriority.first).tap do |i|
    i.assigned_to = assignee
    i.parent_issue_id = parent&.id
    i.save!
  end
end

feature = Tracker.find_by!(name: 'Feature')
cong_no = make_issue(salex, feature, 'Quản lý công nợ khách hàng', ba)
su_kien = make_issue(salex, feature, 'Module Sự kiện', ba)
make_issue(salex, feature, 'Báo cáo doanh số theo vùng', ba)
fe_cong_no = make_issue(salex, task, '[FE] Màn hình danh sách công nợ', dev, assignee: dev, parent: cong_no)
review_su_kien = make_issue(salex, task, 'Review Module Sự kiện', dev, assignee: dev, parent: su_kien)
leave = make_issue(internal, Tracker.find_by!(name: 'Support'), 'Nghỉ phép', ba)

dev_activity = TimeEntryActivity.find_by(name: 'Development') || TimeEntryActivity.first

def log(user, issue, date, hours, activity, comment)
  TimeEntry.new(project: issue.project, issue: issue, spent_on: Date.parse(date), hours: hours,
                activity: activity, comments: comment).tap do |t|
    t.user = user
    t.author = user
    t.save!
  end
end

log(dev, fe_cong_no,     '2026-10-19', 8, dev_activity, 'Màn hình danh sách')
log(dev, fe_cong_no,     '2026-10-20', 3, dev_activity, 'Filter + paging')
log(dev, review_su_kien, '2026-10-20', 2, dev_activity, 'Review PR')
log(dev, leave,          '2026-10-21', 8, dev_activity, 'Nghỉ phép')
log(dev, fe_cong_no,     '2026-10-23', 6, dev_activity, 'Fix bug')
log(dev, review_su_kien, '2026-10-23', 3, dev_activity, 'Review')
log(dev, fe_cong_no,     '2026-10-26', 5, dev_activity, 'Sửa feedback công nợ')
log(dev, review_su_kien, '2026-10-26', 3, dev_activity, 'Review Module Sự kiện')
log(dev, fe_cong_no,     '2026-10-27', 4, dev_activity, 'Sửa feedback công nợ')

puts "Seeded. Leave issue ##{leave.id}. dev1 API key: #{dev.api_key}"
