# Development

Running the app locally, the checks CI runs, how the code is laid out, and
how releases are made.

## Run it locally

You need Rust (stable) and Node 24.

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

## API client

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
  src/sharing.rs   shares and who can see a project (the project_access view)
  src/themes.rs    theme files
  src/projects.rs  projects; links.rs and canvas.rs for the canvas
  src/board.rs     columns and labels
  src/tasks.rs     tasks, checklists, the global board
  src/comments.rs  comments on tasks (a task's log)
  src/dashboard.rs a project's overview in one call
  src/notes/       the notes vault: files and index (vault.rs), markdown and
                   wiki links (markdown.rs), zip import and export (transfer.rs)
  src/databases/   project databases: routes (mod.rs), value rules and filters
                   (values.rs), CSV parsing (csv.rs), import and export
                   (transfer.rs)
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
  src/data/        database views (table, board, calendar), cells, embeds
  src/sharing/     Share dialog, sharing queries and labels
  src/pages/       one file per route
themes/            bundled themes and their JSON schema
```

## Releases

Push a version tag and GitHub does the rest:

```sh
git checkout main && git pull
git tag v1.0.0
git push origin v1.0.0
```

The **Release** workflow runs the same checks as CI, publishes the image as
`1.0.0`, `1.0` and `latest`, and creates a GitHub Release with notes made from
the merged pull requests. A tag with a dash, such as `v1.1.0-rc.1`, becomes a
pre-release and doesn't move `latest`. The image is built for x86-64 (`amd64`)
servers.

The workflow passes the version to the Docker build (`--build-arg VERSION=1.0.0`),
which bakes it into the binary as `TMP_BUILD_VERSION`. The app compares it with
the latest GitHub release to show the owner an update banner. A build without
it is a development build: it reports the `Cargo.toml` version and never shows
the banner. To try the banner locally, build with
`TMP_BUILD_VERSION=0.0.1 cargo run` and point `TMP_UPDATE_CHECK_URL` at any URL
that answers like GitHub's `releases/latest`.

The first time, GitHub makes the package private. Either make it public
(the package page → **Package settings** → **Change visibility**), or sign in
on the server with `docker login ghcr.io` using a personal access token with
the `read:packages` scope.
