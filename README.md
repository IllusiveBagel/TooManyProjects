# too many projects

A self-hosted canvas for every project you have on the go. Projects live as
cards on one infinite canvas, projects that grow out of other projects are
joined by arrows, and each one opens into a dashboard with its tasks and board.

> **Status: phase 0 (foundations).** Accounts, login, invites, the database
> schema, theming and the Docker image are in place. The canvas, projects and
> tasks arrive in phases 1 and 2.

## Run it

```sh
docker compose up -d
```

Open <http://localhost:8080>. The first person to open a new install creates
the **owner** account. After that, people join only through invite links the
owner makes under **Settings → Invites**.

Everything the app stores lives in the `/data` volume: the SQLite database
(`toomanyprojects.db`) and the theme files (`themes/`). Back up that folder and
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
  tests/           API tests against a real SQLite file
web/               React 19, TypeScript, Vite, Tailwind v4
  src/api/         generated OpenAPI types and the typed client
  src/theme/       theme engine: resolves tokens into CSS variables
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
