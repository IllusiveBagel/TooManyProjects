# User guide

How each part of **too many projects** works. For installing it, see the
[README](../README.md#quick-start); for working on the code, see
[development.md](development.md).

- [Today, search and quick add](#today-search-and-quick-add)
- [Canvas groups](#canvas-groups)
- [Boards](#boards)
- [Calendar](#calendar)
- [Notes](#notes)
- [Sharing](#sharing)
- [Project databases](#project-databases)
- [Parts and the Shopping list](#parts-and-the-shopping-list)
- [AI access (MCP)](#ai-access-mcp)
- [Put it online with HTTPS](#put-it-online-with-https)
- [Theming](#theming)
- [Updating](#updating)

## Today, search and quick add

**Today** (`/today`, the first tab on a phone, and where the phone app opens)
answers "what should I touch next?" across every project:

- **Now**: tasks in a column that counts as in progress. **Blocked**: tasks in
  a blocked column. Both list tasks assigned to you, plus unassigned ones on
  your own projects, so a friend's shared board doesn't fill your day.
- **Due in the next 7 days**, with late ones first and marked.
- **Tonight / Today**: your work sessions from the calendar (busy time isn't shown).
- **Gone quiet**: your *active* projects with no activity for 14 days.
  **Pause** sets the project to paused, so it stops showing here.
- **Inbox**: captures not yet filed under a project. **File it** turns one
  into a task (its link goes in the description and its photos move to the
  task) or files it as a note under a project.

Tick a task to move it to its board's first Done column; click it to open it.

**Search** (Ctrl+K, Cmd+K on a Mac, or the magnifier in the top bar) looks
through projects, tasks, comments in task logs, notes and database rows as you
type. Words match the start of words, so `carb` finds *carburettor*. Arrow keys
move, Enter opens, and Tab steps through the filters (tasks, notes, data,
projects). With nothing typed it shows what you opened recently. It also runs
actions: **New task in …** (Alt+Enter, into the project on screen or the one
you last used), **Quick add**, **New note**, **Plan time**, **New project**,
go to any screen or a project's board, notes, data or photos, and switch
between dark and light.

**Quick add** (Alt+A anywhere, **+ Quick add** on Today, **+ Add** on a
phone's Today) takes one line, as a **Task** (into the project's first To do
column), a **Note** filed under the project, or into the **Inbox** to sort
later. It remembers the last project you picked. Paste a link and it fetches
the page's title (the server fetches public addresses only, never ones on
your own network). **Photo** adds pictures, from the camera on a phone.

### Sharing to it from a phone

Install the app on your phone: open it in Chrome and choose **Add to Home
screen** / **Install app** (it needs HTTPS, see below). On Android it then
appears in the share sheet: share a Printables link or a photo to **Projects**
and Quick add opens with it filled in.

iPhones don't let web apps receive shares, so use a Shortcut instead:

1. In the Shortcuts app, make a new shortcut and turn on **Show in Share
   Sheet** (accepting URLs and text).
2. Add **URL** with
   `https://your-server/today?add=1&url=` followed by the **Shortcut Input**
   variable (use **URL Encode** on it first).
3. Add **Open URLs**.

Sharing a page to that shortcut opens Quick add in Safari with the link filled
in. (Photos can't travel this way; use **Photo** in Quick add instead.)

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
- **Columns:** drag a column by the handle (⠿) left of its name, or use its
  **⋯** menu to rename it, change what it counts as, move it left or right
  (**Alt+←/→** on the header does the same), add a column before or after it,
  or remove it (its tasks move to a column you pick first). A move shows
  **Undo** for a few seconds. **Columns** above the board (or under it on a
  phone) opens a list of every column to reorder, rename and recategorise in
  one place. Up to 20 columns.
- **Done columns:** if nothing on a board counts as done, or a column is
  called *Done* (or *Finished*, *Complete*, *Published*) but counts as
  something else, the board says so and offers a one-click fix. **Not now**
  hides the notice until the columns change. Making a column with tasks count
  as done asks first, since its tasks are then finished, dated today.
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

## Photos and files

- **Attach anything** to a task, a comment in a task's log, a database row or
  a note: drop or paste files onto it, or use **Photo** and **Attach**. On a
  phone, **Take photo** opens the camera, and the camera button in the log box
  adds photos to that log entry. Several can go in at once.
- **Photos** show as thumbnails; click one for the full view, where you can add
  a caption, download the original, jump to its task, delete it, or **Use as
  cover on the canvas** so it shows on the project's card. Other files show as
  a name, type and size; PDFs open in the browser and everything else
  downloads.
- **Photos tab:** every project has one. It reads as a build log: newest
  first, grouped by week (by when the photo was taken, from the camera's
  data, else when it was added), each tile saying which task it came from.
  Filter by task, include the projects spawned from it, or hide other files.
  Photos from notes linked to the project show too. On a phone, the big camera
  button adds photos straight to the project.
- **Notes:** paste or drop an image into the editor, or use **Photo** /
  **Attach** above the note. The file goes into your vault's `attachments/`
  folder and an Obsidian embed (`![[photo.jpg]]`) is added, so Obsidian sees
  it too. Embeds and relative image links from an imported vault show in the
  app as well.
- **Databases** get a **Gallery** view: cards led by each row's first photo.
  Add photos from a row's dialog.
- **Limits:** 25 MB per file unless `TMP_MAX_UPLOAD_MB` says otherwise. Any
  file type is kept; JPEG, PNG, WebP and GIF get 400 px and 1600 px previews
  (turned upright, with the camera's location and other EXIF data left out).
  The original is kept untouched.
- **Where they live:** on the data volume, next to the database and vaults:
  `/data/files/<account id>/<year-month>/`, previews in `/data/files/thumbs/`,
  and note attachments in the vault. Back up `/data` and you have everything.
- **Who sees them:** whoever can see the task, row, project or note they are
  on. Editors can add, caption and delete; viewers can look. Files are only
  served to people signed in, never from a public link.
- **Removing** a task, comment or row keeps its photos on the project (or the
  task); deleting a project removes its files from disk.

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

## Project templates

A new project can start from a template instead of a blank board. Pick one under
**Start from** in the New project dialog:

| Template | Columns | Also brings |
| --- | --- | --- |
| Blank | To do, Doing, Done | Nothing else (today's default) |
| Electronics / PCB | Design, Ready to order, Ordered, Assembly, Testing, Done | Parts database, labels Schematic, Layout, Firmware |
| 3D print / prop | Print, Fill, Prime, Mask, Paint, Weather, Done | Parts database, a finishing checklist task |
| Car / restoration | To do, Doing, Waiting on parts (Blocked), Done | Parts database, labels Disassembly, Bodywork, Engine |
| Video | Idea, Script, Shoot, Edit, Published | Labels Episode, Short, B-roll |
| Software | Backlog, Doing, Review, Done | Labels Bug, Feature |

Each column already counts as the right thing (To do, In progress, Blocked or Done), so
the global board and Today read the new project correctly from the start. Everything a
template makes is ordinary afterwards: rename, move or delete any of it.

**Save your own.** Open a project's panel on the canvas and choose **Save as template…**.
Pick what comes along: board columns, labels, open tasks as starter tasks (with their
checklists), database columns and views without rows, and note skeletons (each note's
headings, without the words under them). Your templates appear in the picker marked
*Yours*.

**Manage them** in Settings → Templates: rename, delete, or tick **Shared** so the people
you share everything with can start from it too. Built-in templates can't be edited, but
**Copy a built-in to change it** makes one of your own.

**A project with an empty board** can still switch: **Use a template…** in its panel
replaces the columns and adds the rest. Once a board has tasks it keeps its columns.

Through the MCP server, `create_project` and `spawn_project` take an optional `template`
by name, such as "Electronics / PCB".

## Parts and the Shopping list

For builds that need things bought: one Parts database per project, and one
Shopping list across all of them.

- **Make one:** in a project's **Data** tab, **+ Database → Parts**. It comes
  with Item, Qty, Unit cost, Supplier, Link, Status, For task, Ordered on,
  Arrived on and Notes, and views All, By status (a board), To buy and
  Gallery. Status runs **Need → Ordered → Shipped → Arrived → Fitted**, and a
  new part starts in Need. It's an ordinary database afterwards: rename
  columns, add your own, add views. The Shopping list reads Qty, Unit cost,
  Supplier, Link and Status, so those keep their type (renaming is fine).
- **Shopping list** (`/shopping`, linked from Today when there's something to
  buy): every part in Need on projects you can change, grouped by supplier
  with a total each; suppliers match ignoring case. Tick parts and press
  **Mark ordered** to move them together; Ordered on is set to today. **On the
  way** lists ordered and shipped parts (**Mark shipped**, **Mark arrived**),
  and **Arrived this month** what came in (**Mark fitted**). **Copy links**
  copies a supplier's product links, one per line. A part's name opens its row.
- **Dates fill themselves:** moving a part to Ordered (or further) fills
  Ordered on, and to Arrived or Fitted fills Arrived on, unless you set them.
- **Tasks that wait on parts:** link a part to a task with **For task**. The
  task's card shows *Waiting on 2 parts*, then *Parts in* once they've all
  arrived, and the task's log gets a "Parts arrived" comment. The card doesn't
  move columns by itself.
- **Spend:** a project's Overview (and the Parts database's Data tab) shows
  what's spent so far (ordered, shipped, arrived, fitted) and what's still to
  buy, plus a family total with the projects spawned from it, and the tasks
  still waiting on parts.
- **Currency:** one for the whole install, set by the owner in **Settings →
  Money** (GBP to start). Changing it relabels prices; it doesn't convert them.
- **With Claude:** `get_shopping_list` reads the list, `mark_parts` moves parts
  along, and `add_row` adds a part.

## AI access (MCP)

The server speaks the [Model Context Protocol](https://modelcontextprotocol.io)
at `/mcp`, so an assistant such as Claude can read your projects and notes and,
if you let it, add to them.

- **Connect Claude (web, desktop or phone):** under **Settings → AI access**,
  copy your server address (`https://your-host/mcp`). In Claude, open
  **Settings → Connectors → Add custom connector**, name it TooManyProjects,
  paste the address and press **Add**, then **Connect**. A TooManyProjects
  page opens: sign in if asked, choose **Read only** or **Read and write**,
  and press **Allow**. You're sent back to Claude, connected. Claude reaches
  your server from the internet, so this needs an `https://` address it can
  reach; see [Put it online with HTTPS](#put-it-online-with-https).
- **Connect Claude Code:** it can sign in the same way, and works on a home
  network address too. Signing in needs HTTPS: the server has its own, see
  [HTTPS on your home network](#https-on-your-home-network).

  ```sh
  claude mcp add --transport http toomanyprojects https://your-host/mcp
  ```

  Then type `/mcp` in Claude Code, pick toomanyprojects and choose
  **Authenticate**. A browser tab opens for the same sign-in and Allow.
- **Connected apps:** each app you allowed is listed under **Settings → AI
  access** with when it was last used. **Disconnect** stops it at once. To
  change read only to read and write, disconnect and connect again. A
  connection acts as you, so it sees what you see, shared projects included.
- **How sign-in works:** this is standard MCP authorization (OAuth 2.1). The
  app is its own sign-in server, so there is nothing else to run: apps find it
  through `/.well-known/oauth-protected-resource` and
  `/.well-known/oauth-authorization-server`, register themselves at
  `/oauth/register`, and use the authorization code flow with PKCE. Access
  tokens (`tmp_at_…`) last an hour and refresh tokens (`tmp_rt_…`) 30 days,
  replaced on every use. Only hashes are stored.
- **API tokens:** for scripts and apps that can't sign in, open **Advanced:
  API tokens** under **Settings → AI access** and make one. It acts as you,
  either *read only* or *read and write*. The secret (`tmp_…`) is shown once;
  only a hash is kept. Send it as `Authorization: Bearer tmp_…`, for example
  `claude mcp add … --header "Authorization: Bearer tmp_…"`. Revoke a token
  there and anything using it stops at once.
- **Tools:** `list_projects` (with the canvas groups each project sits in, and
  a `group` filter), `get_project_context` (a full briefing, with the
  family on request), `get_today` (the Today screen as text), `search` (projects, tasks, comments,
  notes and database rows), `list_notes`,
  `get_note`, `get_task` (a task with its log, linked notes, photos and history),
  `list_attachments` (a project's or task's photos and files) and
  `get_attachment` (shows Claude the photo itself, or a small text file),
  `weekly_review` (the week across every project: finished, overdue, due
  next, blocked and stuck tasks, and planned time),
  `query_database` (a saved view or a search as a markdown table, with
  totals), `get_shopping_list` (parts to buy across projects, by supplier),
  and with read-write access `mark_parts` (move parts to ordered, shipped,
  arrived or fitted), `quick_add` (a line into a project or
  the Inbox), `create_task`, `update_task` (move,
  finish, reopen or edit by `T-12` or id), `add_task_comment`,
  `link_note_to_task`, `attach_file` (a file Claude made, such as a CSV or
  cut list, onto a project, task or note), `add_note` (optionally linked to a task), `append_to_note`,
  `add_row` (values by column name), `spawn_project`, `create_project`,
  `update_project` (title, description, status, tags), `update_note` (a new
  title or body, or one `##` section replaced), `add_column` and
  `update_column` (rename, recategorise or move a board column) and
  `plan_time` (a session or busy time, optionally repeating). `create_task`
  can also log work that is already finished, with `done` and `completed_on`,
  and `update_task` takes `completed_on` to finish a task on the day it was really done. `get_calendar` reads a stretch
  of the calendar (a week by default) with who's joining and hours per
  project; busy time shows only for the person connected. Projects and notes
  can be named by title or id.
- **Resources and prompts:** each project (`tmp://projects/{id}`) and note
  (`tmp://notes/{id}`) is a resource, and there are `project_briefing` and
  `weekly_review` prompts. Project briefings list the next two weeks of
  sessions, and the weekly review includes what was planned last week and
  this week, counts a task as stuck when it is in progress with nothing
  logged for 7 days, and ends with suggested changes for you to approve. The
  same review is a `weekly_review` tool, for apps that don't show prompts.
- **History:** anything changed through a connection or token shows as *via AI* in the
  project's activity and the task's history.

## HTTPS on your home network

Claude Code only signs in over HTTPS. The server provides it itself on port
**8443**, with no proxy or tunnel: the first time it starts it makes a small
certificate authority of its own (kept in `/data/tls`) and uses it to sign the
certificate it serves. Each computer trusts that authority once; after that the
certificate renews itself and can cover new addresses without anyone doing
anything again.

1. **Make sure the certificate covers the address you use.** Open
   **Settings → HTTPS**. If it says the certificate doesn't cover the address
   in your browser bar, press **Add** (owner only). Inside Docker the server
   can't see your network's addresses, so add the name or IP you open it at,
   such as `192.168.1.50` or `nas.local`. You can also list them up front with
   `TMP_TLS_HOSTS: "192.168.1.50,nas.local"`.
2. **On each computer that runs Claude Code**, download the certificate from
   **Settings → HTTPS** and tell Claude Code to trust it with the line shown
   there, which sets `NODE_EXTRA_CA_CERTS` to the downloaded file. Open a new
   terminal afterwards.
3. **Add the server** with the `https://…:8443/mcp` address from
   **Settings → AI access**, then `/mcp` → **Authenticate** in Claude Code.

The sign-in page opens in your browser, which warns about the certificate until
you trust it there too (Mac: open the file in Keychain Access and choose
*Always Trust*; Windows: *Install Certificate* → *Trusted Root Certification
Authorities*), or you can continue past the warning.

- **Your own certificate:** if you have one (for example from your own domain),
  set `TMP_TLS_CERT` and `TMP_TLS_KEY` to the certificate (full chain) and key
  files, mounted into the container. Nothing needs installing on other
  computers.
- **Turn it off** with `TMP_HTTPS: "off"`, or move it with
  `TMP_HTTPS: "0.0.0.0:9443"`. Port 8080 keeps serving plain HTTP either way.
- **Keep `/data/tls/ca-key.pem` private.** Anyone with it could make
  certificates that computers trusting this authority would accept. It lives
  in the data volume with everything else, readable only by the app.

## Put it online with HTTPS

The Claude app (on the web, desktop and phone) connects to your server from
Anthropic's cloud, not from your computer. So your server needs an address
that works from anywhere on the internet, over HTTPS. If you only use Claude
Code on your home network, you can skip this: see
[HTTPS on your home network](#https-on-your-home-network).

You need two things:

1. **An `https://` address** that reaches the app's port 8080. Pick one of the
   three ways below.
2. **`TMP_PUBLIC_URL` set to that address** in `docker-compose.yml`, such as
   `TMP_PUBLIC_URL: "https://projects.example.com"`, then
   `docker compose up -d`.

> [!IMPORTANT]
> Once `TMP_PUBLIC_URL` starts with `https://`, sign-in cookies are only sent
> over HTTPS, so open the app at that address from then on. To keep signing in
> at a plain `http://` home address as well, also set
> `TMP_COOKIE_SECURE: "false"`.

### Option 1: Cloudflare Tunnel (you have a domain on Cloudflare)

No ports to open on your router, and Cloudflare handles the certificate.
Free for this.

1. In the Cloudflare dashboard, go to **Zero Trust → Networks → Tunnels →
   Create a tunnel**, choose **Cloudflared**, name it, and copy the token it
   shows.
2. Add `cloudflared` next to the app in `docker-compose.yml`:

   ```yaml
   services:
     toomanyprojects:
       # … as before, with:
       environment:
         TMP_PUBLIC_URL: "https://projects.example.com"
     cloudflared:
       image: cloudflare/cloudflared:latest
       restart: unless-stopped
       command: tunnel --no-autoupdate run
       environment:
         TUNNEL_TOKEN: "paste-the-token-here"
   ```

3. Back in the tunnel's settings, add a **public hostname**:
   `projects.example.com`, service type **HTTP**, URL `toomanyprojects:8080`.
4. `docker compose up -d`.

If you put **Cloudflare Access** in front of the app, Claude can't get past
it. Either leave Access off (the app has its own sign-in) or add a bypass for
the paths `/mcp`, `/oauth/*` and `/.well-known/*`.

### Option 2: Tailscale Funnel (no domain needed)

Gives you a free address like `https://myserver.tail1234.ts.net`.

1. [Install Tailscale](https://tailscale.com/download) on the server and sign
   in.
2. In the Tailscale admin console, under **DNS**, turn on **HTTPS
   certificates**, and make sure your access controls allow **Funnel** (the
   console offers to add it the first time you run the next step).
3. On the server, run:

   ```sh
   sudo tailscale funnel --bg 8080
   ```

   It prints your public address. Set `TMP_PUBLIC_URL` to it.

Use **funnel**, not `tailscale serve`: serve only works inside your tailnet,
and Claude isn't in it.

### Option 3: your own domain and a reverse proxy

If you can forward ports 80 and 443 on your router to the server and point a
domain's DNS at your home IP, [Caddy](https://caddyserver.com) gets and renews
the certificate for you. A whole `Caddyfile`:

```
projects.example.com {
    reverse_proxy localhost:8080
}
```

Any other reverse proxy (nginx, Traefik, Nginx Proxy Manager) works too, as
long as it passes on the `Host` header and sets `X-Forwarded-Proto: https`.
nginx limits request bodies to 1 MB by default, which stops photo uploads: set
`client_max_body_size 30m;` (or more, to match `TMP_MAX_UPLOAD_MB`).

### Check it works

From your phone on mobile data (not Wi-Fi), open
`https://your-address/.well-known/oauth-protected-resource`. You should see a
few lines of text whose `resource` is your `https://` address followed by
`/mcp`. Then go to **Settings → AI access** at that address: the warning about
Claude not reaching it is gone, and you can connect.

Putting the app online makes its sign-in page public. Accounts are invite-only
and passwords are at least 10 characters, but use a strong one.

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
