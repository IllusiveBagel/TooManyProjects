# User guide

How each part of **too many projects** works. For installing it, see the
[README](../README.md#quick-start); for working on the code, see
[development.md](development.md).

- [Canvas groups](#canvas-groups)
- [Boards](#boards)
- [Calendar](#calendar)
- [Notes](#notes)
- [Sharing](#sharing)
- [Project databases](#project-databases)
- [AI access (MCP)](#ai-access-mcp)
- [Theming](#theming)
- [Updating](#updating)

## Canvas groups

A group is a labelled, coloured area on the canvas, such as *Electronics*,
*Car projects* or *Programming*. Grab it by its label and everything inside
moves with it. (Not to be confused with *blocks* links, which say one project
holds up another.)

- **Make one:** press **Group** at the top left, then drag out the area, or
  tap for a standard-sized one, and type its name. To wrap cards you already
  have, select two or more first by holding Shift and dragging a box round them,
  then press **Group**. Escape or **Cancel** stops drawing.
- **What's inside:** a card is in a group while its centre is inside the area.
  There's nothing to tick: drag a card in to add it, drag it out to take it
  out. The number on the label is how many cards are inside.
- **Move:** drag the label. The cards inside, and any smaller group drawn
  wholly inside it, come too. What it carries is fixed when you start
  dragging, so cards it passes over on the way are left where they are.
- **Resize, rename, recolour, remove:** click the label to select the group.
  Drag a corner or edge to resize (the cards stay put), and use the bar above
  it to rename (or double-click the label), pick one of the eight theme
  colours, or remove it. Removing a group never touches the projects in it.
- **Overlaps and nesting:** groups can overlap and sit inside each other.
  Smaller ones draw on top, so their labels stay reachable. A card in two
  overlapping groups moves with whichever you drag.
- **Inside a group** the canvas still pans and box-selects as normal; only the
  label and the resize handles pick the group up.
- **Phones:** the same, with bigger resize handles, and the group's tools in a
  bar above the tabs. A new group zooms in so you can read its name as you
  type it.
- **Sharing:** groups are part of your own canvas layout, like card positions,
  so nobody else sees them. You can put projects shared with you in your
  groups, and people you share with arrange their own.
- **AI access:** `list_projects` ends with the groups and the projects in
  each, and takes a `group` filter, so you can ask about "my electronics
  projects".

## Boards

- **Project board** (`/projects/:id/board`): drag cards between and within
  columns. Every column counts as *to do*, *in progress*, *blocked* or *done*,
  which is what the stats, the canvas cards and the global board read.
- **Task drawer**: click a card to edit its title, column, due date,
  priority, labels, notes and checklist, and to see its history. The open
  task is in the URL (`?task=`), so it can be linked.
- **Log:** each task has a log of comments (markdown) for what you did, tried
  and learnt. Owners and editors write; viewers read. You can edit your own
  comments, and a project's owner can delete anyone's. Cards show the count
  (✎).
- **Linked notes:** link vault notes to a task from the drawer (**+ Link a
  note**), or start a new one there (**+ New note**, filed under the task's
  project). A note shows its tasks in its side panel, where you can link more.
  Linking doesn't share a note: someone who can't read it sees only that a note
  is linked. Cards show the count (¶).
- **Global board** (`/board`): four fixed columns. Each project's columns fold
  into them by category, and dropping a card moves it to that project's first
  column of the category (adding one if the project has none). Filter by one
  project or by a project and everything spawned from it.

## Calendar

The **Calendar** tab is your week, Monday first, for giving projects real
time.

- **Work sessions** are time on one project, with an optional focus such as
  "Rear brakes". They're drawn in the project's colour, and everyone who can
  see the project sees them. You can plan sessions on projects you own or can
  edit.
- **Busy time** is time you can't give to projects: work, appointments, a
  family day. It's hatched grey and **only you ever see it**. Nobody else is
  told you're busy; it only counts against your own free hours.
- **Repeats:** either kind can repeat weekly on the days you tick (Weekdays
  and Weekends are one click), with an optional end date. Times are kept as
  local wall-clock time plus your time zone, so 9 to 5 stays 9 to 5 when the
  clocks change. Changing a repeat changes every week. To drop one week, open
  it and choose **Delete this one**; **Every week** removes the series.
- **Planning:** drag across empty time on the week (or tap it on a phone),
  use **+ Plan time** in the top bar, or **Plan** beside a project under
  *No time yet*. Drag a one-off session to move it, or its bottom edge to
  change its length; open a repeating one to change it.
- **This week** adds up the hours you've planned per project and the hours
  still free between 07:00 and 22:00 once busy time and sessions are taken
  out. **No time yet** lists active projects with nothing planned this week.
- **Joining:** a session someone else planned on a project shared with you
  shows dashed. Open it and press **I'll join**; you join that one week (even
  on a repeat), and the planner sees you on the session and in the project's
  activity. Press it again to take it back. Viewers can join but not plan.
- **Everyone / Just mine** switches between every session you can see and
  only those you planned or are joining.
- **On a phone** the calendar shows one day at a time with a Monday-to-Sunday
  strip, and the week's totals as chips above it.
- Each project's **Overview** has a *Coming up* panel with its next sessions
  over two weeks and a **+ Plan session** button.

## Notes

- **Your vault is plain files.** Each person's notes are markdown files under
  `/data/vaults/<account id>/`, with YAML front matter for `id`, `tags`,
  `projects`, `created` and `updated`. The database only indexes them, so you
  can point Obsidian (or any editor, or a sync tool) at that folder. The app
  rescans every 15 seconds and on **Settings → Notes → Rescan vault**.
- **Folders:** `Inbox/` for quick notes, `Knowledge/` for everything general,
  and `Projects/<project title>/` for notes filed under a project. Any other
  folders you make are kept.
- **Links:** `[[Note title]]`, `[[Note title|shown text]]`, `[[Note#heading]]`,
  and `[[@Project title]]` for a project. Renaming a note or a project rewrites
  the links to it. A link to a note that doesn't exist yet creates it when
  clicked.
- **Notes screen** (`/notes`): folders and tags on the left, search with
  Ctrl K, a note opens rendered with an **Edit** toggle (Ctrl E) and autosave.
  Typing `[[` suggests notes, `[[@` suggests projects. Each project has a
  **Notes** tab listing its notes and the notes that mention it.
- **Quick note:** Alt N anywhere (or **+ New** on the Notes screen on a
  phone) drops a note into the Inbox, or straight under a project.
- **Import and export** (**Settings → Notes**): import a zipped Obsidian vault
  or a Notion *Markdown & CSV* export (page ids are stripped and links turned
  into wiki links; Notion databases are skipped here: import their CSV from a
  project's **Data** tab instead).
  Nothing is overwritten: a clashing name gets a number. Export downloads the
  whole vault as a zip.

## Sharing

Everyone on a server has their own canvas, projects and vault. Share with
someone who already has an account here, or invite them.

- **One project:** open it and use **Share**. Pick a person and *Can edit* or
  *View only*, and whether projects spawned from it come too (on by default).
  The dialog lists everyone who can see the project and why: owner, shared
  this project, shared everything, or through a parent project.
- **Everything:** **Settings → Sharing** shares every project you have, now and
  later, with *Can edit* or *View only*. Tick the box to include your knowledge
  notes as well, not only project notes. The same panel lists what you share
  and what's shared with you, where you can change a role, stop sharing, or
  leave.
- **Viewers** see the board, tasks and notes but can't change them. **Editors**
  can do everything except delete the project or change who has it. A project
  an editor spawns from a shared project belongs to its owner, and the editor
  keeps access to it.
- **Canvas:** shared projects sit on your canvas with the owner's initials
  (dashed for view only), and you lay them out yourself. A link to a project
  you can't see shows a *Private project* card, so the arrow still makes sense.
- **Notes follow projects:** a note filed under a shared project is shared with
  it, under **Notes → Shared with me**. Editors edit it in place in the owner's
  vault; only the owner can delete it.
- **Assignees:** a task can be assigned to anyone who can see its project, and
  **My tasks** on the global board shows yours.
- **Invites:** the server's owner can make an invite that also shares
  everything, or (from a project's Share dialog) one project.
- **AI access:** a token sees what its person sees, shared projects included.

## Project databases

Each project has a **Data** tab with tables you define, like a parts list,
an episode tracker or a budget.

- **Columns:** text, number, currency, select, multi-select, date, checkbox,
  URL, person, and relation (to tasks, notes, projects or rows of another
  database). The first column is the row's name. Changing a column's type
  converts what it can, and text turned into a select keeps its values as
  choices.
- **Views:** a table (edit cells in place, totals under number and currency
  columns), a board with a lane per choice of a select column, or a calendar by
  a date column. Each view keeps its own filters, sorts and hidden columns.
  Click **↗** on a row for the whole row as a form.
- **Pin to overview:** a pinned view shows live on the project's overview.
- **In notes:** `![[Parts list#To order]]` on its own line embeds that view,
  live and editable; type `/database` in the editor to pick one.
- **In tasks:** rows that link to a task show in its drawer with their total,
  where you can link another row or add one.
- **CSV:** **+ Database → Import CSV** takes a Notion database export (or any
  CSV) and guesses the column types. **Export** downloads a zip with the CSV and
  one markdown file per row.
- **Limits:** 50 databases a project, 60 columns, 20 views and 10,000 rows a
  database.

## AI access (MCP)

The server speaks the [Model Context Protocol](https://modelcontextprotocol.io)
at `/mcp`, so an assistant such as Claude can read your projects and notes and,
if you let it, add to them.

- **Tokens:** make one under **Settings → AI access**. It acts as you, and is
  either *read only* or *read and write*. The secret (`tmp_…`) is shown once;
  only a hash is kept. Revoke a token there and anything using it stops at
  once.
- **Connect Claude Code:**

  ```sh
  claude mcp add --transport http toomanyprojects https://your-host/mcp \
    --header "Authorization: Bearer tmp_…"
  ```

  Other clients that let you set a header take the JSON config shown when you
  make a token. The token goes in an `Authorization: Bearer` header; OAuth sign
  in isn't supported yet, so connectors that only do OAuth (such as custom
  connectors in the Claude web app) can't connect.
- **Tools:** `list_projects` (with the canvas groups each project sits in, and
  a `group` filter), `get_project_context` (a full briefing, with the
  family on request), `search` (projects, tasks and their comments, and notes), `list_notes`,
  `get_note`, `get_task` (a task with its log, linked notes and history),
  `query_database` (a saved view or a search as a markdown table, with
  totals), and with a read-write token `create_task`, `update_task` (move,
  finish, reopen or edit by `T-12` or id), `add_task_comment`,
  `link_note_to_task`, `add_note` (optionally linked to a task), `append_to_note`,
  `add_row` (values by column name), `spawn_project` and `plan_time` (a
  session or busy time, optionally repeating). `get_calendar` reads a stretch
  of the calendar (a week by default) with who's joining and hours per
  project; busy time shows only for the token's owner. Projects and notes
  can be named by title or id.
- **Resources and prompts:** each project (`tmp://projects/{id}`) and note
  (`tmp://notes/{id}`) is a resource, and there are `project_briefing` and
  `weekly_review` prompts. Project briefings list the next two weeks of
  sessions, and the weekly review includes what was planned last week and
  this week.
- **History:** anything changed through a token shows as *via AI* in the
  project's activity and the task's history.

## Theming

Every colour, font, radius and shadow comes from a theme file. The default,
**Workshop** (`themes/workshop.json`), has a dark mode and a light **Paper**
mode. Tokens come in three layers:

- **Primitives**, raw values like `graphite-900: #121110`. Only the theme file refers to them, as `{graphite-900}`.
- **Semantic** tokens components use: `surface`, `surface-card`, `line`, `text`, `text-quiet`, `accent`, `late` and so on, plus project colour **slots 1 to 8**.
- **Component** tokens for signature details: card shadows, the canvas grid, the calendar's busy hatching (`calendar-busy`) and hour lines (`calendar-hour`).

The web app turns the active theme into `--tmp-*` CSS variables, and Tailwind's
`@theme` maps utilities onto them (`bg-surface-card`, `text-accent`,
`shadow-card`). Tailwind's own palette is switched off, so components cannot
use a raw colour. Projects store a slot (`slot-3`), never a hex code, so a new
theme recolours everything at once.

To add a theme, copy `workshop.json` into `/data/themes/` under a new `id` and
restart. It then appears under **Settings → Appearance**. Fonts are bundled
with the app; nothing loads from the internet.

## Updating

When a newer release is out, the owner sees a thin banner under the top bar
with links to **What's new** (the release notes) and back to this section.
Closing it hides that version; the next release brings it back. Other
accounts never see it. **Settings → Updates** shows the running version, what
the last check found and a **Check now** button.

The server asks GitHub for the latest release when it starts and every six
hours after. It is one anonymous request and sends nothing about your install.
Pre-releases are ignored. To turn the check off, for an offline server or
because you'd rather not, set `TMP_UPDATE_CHECK=off`. Images you build yourself
without a version count as development builds and never show the banner.

To update with Docker Compose:

```sh
docker compose pull && docker compose up -d
```

In **Portainer**, open the stack, choose **Update the stack** and turn on
**Re-pull image and redeploy** (or open the container, choose **Recreate** and
turn on **Re-pull image**). Your data lives
in the `/data` volume, so it carries over. If you pinned a version such as
`:1.2.0` in the compose file, change it to the new one first. Database changes
run on their own when the new version starts; back up `/data` first if you
want a way back.
