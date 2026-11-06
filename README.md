# too many projects

A self-hosted canvas for every project you have on the go. Projects live as
cards on one infinite canvas, projects that grow out of other projects are
joined by arrows, and each one opens into a dashboard with its tasks and board.

> **Status: phase 3b (MCP server).** Accounts, invites, theming, the canvas
> with spawned, related and blocking links, tasks with per-project and global
> boards, a notes vault of markdown files you can open in Obsidian, and now an
> MCP server so an AI assistant can read and update your projects. Sharing and
> project databases come next.

### Boards

- **Project board** (`/projects/:id/board`): drag cards between and within
  columns. Every column counts as *to do*, *in progress*, *blocked* or *done*,
  which is what the stats, the canvas cards and the global board read.
- **Task drawer**: click a card to edit its title, column, due date,
  priority, labels, notes and checklist, and to see its history. The open
  task is in the URL (`?task=`), so it can be linked.
- **Global board** (`/board`): four fixed columns. Each project's columns fold
  into them by category, and dropping a card moves it to that project's first
  column of the category (adding one if the project has none). Filter by one
  project or by a project and everything spawned from it.

### Notes

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
  into wiki links; Notion databases are skipped until project databases land).
  Nothing is overwritten: a clashing name gets a number. Export downloads the
  whole vault as a zip.

### AI access (MCP)

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
- **Tools:** `list_projects`, `get_project_context` (a full briefing, with the
  family on request), `search` (projects, tasks and notes), `list_notes`,
  `get_note`, and with a read-write token `create_task`, `update_task` (move,
  finish, reopen or edit by `T-12` or id), `add_note`, `append_to_note` and
  `spawn_project`. Projects and notes can be named by title or id.
- **Resources and prompts:** each project (`tmp://projects/{id}`) and note
  (`tmp://notes/{id}`) is a resource, and there are `project_briefing` and
  `weekly_review` prompts.
- **History:** anything changed through a token shows as *via AI* in the
  project's activity and the task's history.

## Run it

```sh
docker compose up -d
```

Open <http://localhost:8080>. The first person to open a new install creates
the **owner** account. After that, people join only through invite links the
owner makes under **Settings → Invites**.

Everything the app stores lives in the `/data` volume: the SQLite database
(`toomanyprojects.db`), the theme files (`themes/`) and everyone's notes
(`vaults/`). Back up that folder and
you have backed up everything.

### Settings

| Variable | Default | What it does |
| --- | --- | --- |
| `TMP_PUBLIC_URL` | unset | The address people reach the app at, such as `https://projects.example.com`. Used for invite links. When it starts with `https://`, session cookies are marked secure. |
| `TMP_COOKIE_SECURE` | from `TMP_PUBLIC_URL` | Force secure cookies on (`true`) or off (`false`). |
| `TMP_SESSION_DAYS` | `30` | How long a sign-in lasts. |
| `TMP_DATA_DIR` | `/data` in Docker | Where the database and themes live. |
| `TMP_BIND` | `0.0.0.0:8080` | Address and port to listen on. |
| `RUST_LOG` | `toomanyprojects=info` | Log level. |

Put the app behind your usual reverse proxy for HTTPS.

## Develop

You need Rust (stable) and Node 22.

```sh
# Terminal 1: API on :8080, data in server/data
cd server && cargo run

# Terminal 2: web app on :5173 with hot reload, proxying /api to :8080
cd web && npm install && npm run dev
```

Checks, as CI runs them:

```sh
cd server && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd web && npm test && npm run build
```

### API client

The server describes its API with OpenAPI (utoipa). The web app's types are
generated from it, so the two cannot drift. After changing an endpoint:

```sh
cd web && npm run api:generate
```

CI fails if `web/src/api/openapi.json` or `schema.d.ts` is out of date. The
live description is also served at `/api/openapi.json`.

## Layout

```
server/            Rust: Axum, SQLx, SQLite
  migrations/      SQL migrations, run automatically on start-up
  src/auth.rs      setup, login, sessions, preferences
  src/invites.rs   invite links
  src/themes.rs    theme files
  src/projects.rs  projects; links.rs and canvas.rs for the canvas
  src/board.rs     columns and labels
  src/tasks.rs     tasks, checklists, the global board
  src/dashboard.rs a project's overview in one call
  src/notes/       the notes vault: files and index (vault.rs), markdown and
                   wiki links (markdown.rs), zip import and export (transfer.rs)
  src/tokens.rs    API tokens for AI assistants
  src/mcp/         the MCP server: tools, resources, prompts (mod.rs) and the
                   markdown briefings they return (brief.rs)
  tests/           API tests against a real SQLite file
web/               React 19, TypeScript, Vite, Tailwind v4
  src/api/         generated OpenAPI types and the typed client
  src/theme/       theme engine: resolves tokens into CSS variables
  src/canvas/      the canvas, cards and links (React Flow)
  src/tasks/       boards (dnd-kit), task cards and the task drawer
  src/notes/       note editor (CodeMirror 6), rendering, quick note
  src/pages/       one file per route
themes/            bundled themes and their JSON schema
```

## Theming

Every colour, font, radius and shadow comes from a theme file. The default,
**Workshop** (`themes/workshop.json`), has a dark mode and a light **Paper**
mode. Tokens come in three layers:

- **Primitives**, raw values like `graphite-900: #121110`. Only the theme file refers to them, as `{graphite-900}`.
- **Semantic** tokens components use: `surface`, `surface-card`, `line`, `text`, `text-quiet`, `accent`, `late` and so on, plus project colour **slots 1 to 8**.
- **Component** tokens for signature details: card shadows, the canvas grid.

The web app turns the active theme into `--tmp-*` CSS variables, and Tailwind's
`@theme` maps utilities onto them (`bg-surface-card`, `text-accent`,
`shadow-card`). Tailwind's own palette is switched off, so components cannot
use a raw colour. Projects store a slot (`slot-3`), never a hex code, so a new
theme recolours everything at once.

To add a theme, copy `workshop.json` into `/data/themes/` under a new `id` and
restart. It then appears under **Settings → Appearance**. Fonts are bundled
with the app; nothing loads from the internet.
