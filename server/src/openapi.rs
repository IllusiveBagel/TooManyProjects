use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "TooManyProjects API",
        description = "JSON API used by the web app. Every path is under /api/v1.",
    ),
    servers((url = "/api/v1")),
    tags(
        (name = "auth", description = "Setup, login and the signed-in person"),
        (name = "invites", description = "Invite links, the only way to join"),
        (name = "themes", description = "Design token files"),
        (name = "projects", description = "Projects, the cards on the canvas"),
        (name = "links", description = "Spawned, related and blocking links between projects"),
        (name = "canvas", description = "Card positions and everything the canvas draws"),
        (name = "board", description = "Columns and labels on a project's board"),
        (name = "tasks", description = "Tasks, checklists, the global board and project dashboards"),
        (name = "notes", description = "Markdown notes in your vault, search, backlinks, import and export"),
        (name = "databases", description = "Project databases: columns, rows, views, import and export"),
        (name = "tokens", description = "API tokens for AI assistants using the MCP server"),
        (name = "system", description = "Health, the running version and update checks"),
    )
)]
pub struct ApiDoc;
