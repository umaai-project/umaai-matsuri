# Issue 追踪器：GitHub

本仓库的 Issue 与需求规格均托管在 GitHub Issues 中，所有操作统一使用 `gh` CLI。

## 操作约定

- **创建 Issue**：`gh issue create --title "..." --body "..."`。多行正文使用 heredoc。
- **读取 Issue**：`gh issue view <number> --json number,title,body,labels,comments`。
- **列出 Issue**：`gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'`，并按需配合 `--label` 与 `--state` 过滤。
- **将 Issue 设为父任务的子 Issue**：`gh issue create --parent <parent> ...`，或创建后执行 `gh issue edit <parent> --add-sub-issue <child>`（需 `gh` 2.94+）。旧版本 `gh` 可用：`gh api --method POST repos/<owner>/<repo>/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>`（使用数据库 ID，见下述**阻塞关系**）。若未开启子 Issue 功能，则在子 Issue 正文顶部写 `Part of #<parent>`。
- **阻塞关系（Blocking）**：优先使用 GitHub 原生的 **issue dependencies**（在 UI 中可见的标准表示）。通过 `gh issue create --blocked-by <n>,<n> ...` 或 `gh issue edit <child> --add-blocked-by <blocker>` 添加依赖（需 `gh` 2.94+）。旧版本 `gh` 可用：`gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`，其中 `<blocker-db-id>` 是阻塞方 Issue 的数字**数据库 ID**（通过 `gh api repos/<owner>/<repo>/issues/<n> --jq .id` 获取，*不是* `#number` 或 `node_id`）。若原生依赖不可用，则退回在子 Issue 正文顶部写 `Blocked by: #<n>, #<n>`。当所有阻塞项均关闭时，该任务即解除阻塞。
- **统计未关闭的阻塞项**：`gh api repos/<owner>/<repo>/issues/<n> --jq .issue_dependencies_summary.blocked_by`（仅统计未关闭的阻塞项；为 0 表示可开始）。`gh issue view <n> --json blockedBy` 也会返回已关闭的阻塞项，需过滤：`--jq '[.blockedBy.nodes[] | select(.state == "OPEN")] | length'`。
- **评论 Issue**：`gh issue comment <number> --body "..."`。
- **添加 / 移除标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`。
- **关闭 Issue**：先用 `gh issue comment <number> --body "..."` 说明原因，再执行 `gh issue close <number>`。对于已关闭的 Issue，`gh issue close --comment` 会忽略评论内容但仍返回退出码 0。

在克隆仓库内执行 `gh` 时会自动从 `git remote -v` 推断目标仓库。

只向预期目标仓库提交 Issue。由于本仓存在 fork 关系（`origin` 与 `upstream`），`gh` 可能默认指向上游仓库；每次会话首次执行 `gh issue create` 前，先用 `gh repo view --json nameWithOwner` 确认目标仓库，若非预期目标需先向用户确认。

## 将 Pull Request 作为分诊入口

**PRs as a request surface: no.** _（若本仓库将外部 PR 视为需求入口，请改为 `yes`；`/triage` 会读取此标记。）_

当设置为 `yes` 时，PR 沿用与 Issue 相同的标签和状态流转，使用对应的 `gh pr` 命令：

- **读取 PR**：先执行 `gh pr view <number>`，再执行 `gh pr view <number> --comments`（非 TTY 下 `--comments` 只输出评论，不输出正文），查看变更使用 `gh pr diff <number>`。
- **列出待分诊的外部 PR**：`gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`。
- **评论 / 打标 / 关闭**：`gh pr comment`、`gh pr edit --add-label`/`--remove-label`、`gh pr close`。

GitHub 的 Issue 与 PR 共用同一套编号空间，因此单纯的 `#42` 可能是其中之一：优先尝试 `gh pr view 42`，失败后再回退到 `gh issue view 42`。

## 当技能要求“publish to the issue tracker”时

创建一个 GitHub Issue。

## 当技能要求“fetch the relevant ticket”时

按上述**读取 Issue**的方式获取内容。

## Wayfinding 操作约定

供 `/wayfinder`（见 [wayfinder.md](wayfinder.md)）使用。**地图（map）**是一个主 Issue，**子任务（child）**作为其子 Issue。

- **地图（Map）**：单个带有 `wayfinder:map` 标签的 Issue，正文包含 Notes / Decisions-so-far / Fog。创建命令：`gh issue create --label wayfinder:map`。
- **子任务（Child ticket）**：作为 GitHub 子 Issue 关联到地图（见**将 Issue 设为父任务的子 Issue**）。若未启用子 Issue，则将子任务添加到地图正文的任务列表中，并在子 Issue 正文顶部写 `Part of #<map>`。标签格式：`wayfinder:<type>`（`research` / `prototype` / `grilling` / `task`）。认领后将任务指派给当前开发者。
- **阻塞关系**：见上文**操作约定**中的**阻塞关系**。
- **前沿查询（Frontier query）**：列出地图下未关闭的子任务（`gh issue list --state open`，限定在地图的子 Issue / 任务列表范围内），剔除存在未关闭阻塞项（见**统计未关闭的阻塞项**，或 `Blocked by` 行中仍未关闭的 Issue）或已有指派人的任务；按地图中的顺序取第一个。
- **认领（Claim）**：`gh issue edit <n> --add-assignee @me`，作为会话的首次写入操作。
- **解决（Resolve）**：`gh issue comment <n> --body "<answer>"`，然后 `gh issue close <n>`，最后将上下文摘要（要点 + 链接）追加到地图的 Decisions-so-far 中。
