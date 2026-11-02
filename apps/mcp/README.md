# OpenCut MCP server

A [Model Context Protocol](https://modelcontextprotocol.io) server that lets
AI agents edit video with OpenCut. It speaks MCP over stdio and drives the
editor through `apps/headless`, so the agent works on real projects that open
in the UI afterwards.

```sh
cd apps/mcp
bun install
bun run start            # stdio server
```

Client configuration (Claude Desktop, Claude Code, Cursor and friends):

```json
{
  "mcpServers": {
    "opencut": {
      "command": "bun",
      "args": ["run", "C:/path/to/OpenCut/apps/mcp/src/index.ts"],
      "env": {
        "OPENCUT_URL": "http://localhost:5173",
        "OPENCUT_PROFILE": "C:/Users/me/.opencut/agent-profile"
      }
    }
  }
}
```

`OPENCUT_URL` is optional; without it the server serves the production build
of `apps/web` itself. `OPENCUT_HEADED=1` shows the browser.

## Tools

| Tool | What it does |
| --- | --- |
| `opencut_list_projects` | Projects stored in the profile |
| `opencut_open_project` | Open by id, or create (`name`, `fps`, `canvas`); later tools act on it |
| `opencut_import_media` | Import files (absolute paths or URLs) under aliases |
| `opencut_add_clip` | Place media on the timeline (`media` alias or id, `at`, `duration`, `trackType`) |
| `opencut_add_text` | Add a title (`text`, `at`, `duration`, `params`) |
| `opencut_get_timeline` | Tracks and elements with times in seconds |
| `opencut_call` | Any Editor API method by dotted name (`timeline.split`, `history.undo`, …) |
| `opencut_run_script` | Run JavaScript in the editor with `opencut` bound, same as the Scripting tab |
| `opencut_export` | Render to an MP4/WebM at an absolute path |
| `opencut_render_job` | Whole job object (same format as headless job files) in one call |

Resource `opencut://api/methods` lists every Editor API method for `opencut_call`.

Media paths must be absolute; the server reads them from disk and streams them
into the page. Exports are written where the agent asks and the path is
returned.
