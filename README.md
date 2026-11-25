# taiq_rust

Rust agents for ForgeIQ. The config loader reads the existing ForgeIQ YAML and
its imports in place. The default build is a headless agent worker: it includes
agents, models, toolsets, connectors, filestores, and credentials, with no UI or
administration modules. The desktop and terminal interfaces are separate build
features.

## Headless agent

```powershell
cargo run --no-default-features --bin taiq-agent -- check
cargo run --no-default-features --bin taiq-agent -- test -c
cargo run --no-default-features --bin taiq-agent -- chat -a IQ -q "Hello"
cargo run --no-default-features --bin taiq-agent -- chat -i prompts.json
'{"id":1,"prompt":"Hello"}' | cargo run --no-default-features --bin taiq-agent -- -f D:\git\orqestra\forgeiq\config\config.yaml chat -a IQ
```

`check` validates imports, agent definitions, model selection, instruction files,
and toolset manifests without contacting providers. `test` probes enabled model
providers through their model-list endpoints; `test -c` also probes enabled
connectors. It reports each result as JSON and exits with an error if a probe
fails. Unsupported provider types are reported as such.

`chat -q` sends one prompt. `chat -i` accepts a text file as one prompt, a JSON
array or `{ "prompts": [...] }`, or a JSON Lines file. Without `-q` or `-i`,
`chat` reads one plain-text prompt or JSON object
`{"id":1,"prompt":"..."}` per stdin line and writes one JSON response per line.
The same ADK session handles every line until EOF. `-f` or `TAIQ_CONFIG` selects
the config file; `chat -a` or `TAIQ_AGENT` selects an agent. Otherwise Taiq uses
the enabled agent marked `default`, then the first enabled agent. For local development, the
config defaults to `../../orqestra/forgeiq/config/config.yaml` relative to this
crate. Set `FORGEIQ_PROFILES` to a comma-separated list to enable profile
overlays. Model credentials come from environment variables, ForgeIQ config,
or the OS credential store where supported. The process keeps session history
in memory; restarting it starts a new session.

The terminal interface is an optional binary:

```powershell
cargo run --features terminal-ui --bin taiq -- ui -a IQ
```

## Docker

```powershell
docker build -t taiq-agent .
$forgeiq = 'D:\git\orqestra\forgeiq'
'{"id":1,"prompt":"Hello"}' | docker run --rm -i --mount "type=bind,source=$forgeiq,target=/forgeiq" -e TAIQ_AGENT=IQ -e OPENAI_API_KEY taiq-agent
```

Mount the complete ForgeIQ project at `/forgeiq` so imported YAML, instructions,
skills, toolset manifests, and local artifacts remain available without copying
them into the image. Use a writable mount if an agent may create artifacts.
Pass provider tokens through container environment variables or a secret mount.
The image exposes no port or web server.

The ADK Rust adapter supports OpenAI, Gemini, OpenRouter, Anthropic, and Ollama.
OpenAI models named `openai/responses/<model>` use the Responses API. A model
entry can override that choice with `use_responses: true` or `false`. For newer
OpenAI reasoning models that use Chat Completions with tools, Taiq sets
`reasoning_effort` to `none` when supported; models that cannot do this require
Responses. The configured `reasoning_default` is used on the Responses API.
When an agent lists multiple models, or selects `auto`, Taiq tries configured
models in order and falls back if a provider fails before its first response.
The headless build includes OpenDAL adapters for S3, GCS, Azure Blob, and
WebDAV. Artifact and file search tools use the configured default filestore.
It loads configured instructions and local skills, registers native Rust versions
of the artifact and workspace search tools from ForgeIQ toolset manifests, and
gives manager agents an `ask_agent` delegation tool. Python implementations of
the other manifest tools are still unavailable in the Rust process. Model tokens
can come from environment variables, ForgeIQ config, or Windows Credential Manager.

## Desktop workspace

Two native renderers share one UI controller and the same file and chat events:
Their default palette mirrors ForgeIQ's web theme. Both bundle Inter (under the
SIL Open Font License in `src/taiq/ui/common/fonts/OFL.txt`) and reuse the
ForgeIQ mark from the web app. Colors and assets live under `ui/common`.

```powershell
cargo build --features desktop-egui,desktop-gpui
.\target\debug\taiq-egui.exe ui
.\target\debug\taiq-gpui.exe -f D:\git\orqestra\forgeiq\config\config.yaml ui -a IQ
.\target\debug\taiq-egui.exe check
.\target\debug\taiq-gpui.exe test -c
```

The GPUI renderer uses the `gpui-ce` package under the `gpui` import name so
it can build on Windows. The desktop executables expose the same `check`,
`test`, and `chat` commands as `taiq-agent`; `ui` starts their renderer.
Launching a desktop executable without a command also starts its UI.

Both accept `-f` to select the ForgeIQ config. Each renderer has a navigation
rail, resizable file and editor panels, chat sessions, reorderable document and
center tabs, and a changes view. Chat history and panel preferences are saved under the user's
local application data directory in `taiq`. The composer supports model
selection, session memory with a clear-context action, a persisted and reorderable
prompt queue, cancellation, the open file, and
text-file attachments. Local filestores include ForgeIQ's read-only Samples
store. You can add a local workspace folder from either UI; the selection is
restored on launch. The file panel supports recursive search, folders, rename/move, delete,
and native import/export dialogs. For a system filestore, the desktop
Documents root uses ForgeIQ's user/workspace/project scope instead of displaying
the physical store directory. Set `TAIQ_USER_ID`, `TAIQ_WORKSPACE_ID`, and
`TAIQ_PROJECT_ID` before launching to open a particular scope; the defaults
are `guest/default/default`. Shared filestores such as Samples use their
configured roots. The editor supports syntax highlighting in egui,
highlighted code previews in GPUI, and source/preview/split modes for
Markdown, code, CSV, JSON, and notebooks, floating document views with saved
bounds, and JSON formatting. Pane sizes, tab order, open files, and light/dark theme choice
persist across launches. Narrow windows can switch between chat and the editor.
Chat renders Markdown headings, lists, quotes, code blocks, and inline styles;
it distinguishes tool calls, results, reasoning, and approval requests with
approve/deny actions, and queued prompts can be removed. Both desktop renderers bind
Ctrl+S to save the open file, Ctrl+N to start a chat, Ctrl+W to close the
open editor tab, and Ctrl+Tab to cycle center tabs.

The Settings rail exposes editable desktop overrides for agents, connectors,
models, plus filestores and direct resource enable controls. Workflows can run manually, interval schedules run while the app is
open, imported ForgeIQ eval sets can run against a selected agent, and local
knowledge records can be searched. Credentials are stored in Windows Credential
Manager; only their metadata is saved in desktop state. The Connectors rail can
browse configured MCP resources/tools, filesystem and cloud storage connectors,
GitHub/GitLab repositories, Brave Search, and Tavily, and read
connector text resources with a `read <path>` query. GitLab paths use
`project:folder/file`; GitHub paths use `owner/repo/folder/file`. Configured
connector tokens or a matching vault credential are used when available.
The Files rail can select configured S3, GCS, Azure Blob, WebDAV, and memory
stores. Remote listing has three bounded attempts; all remote operations have
a timeout. Filestore secrets entered in Settings are placed in the OS vault
and omitted from desktop override files.
Overrides are saved in the desktop data directory; ForgeIQ's imported YAML files
are not rewritten. The Web tab loads HTTP/HTTPS documents asynchronously and
lets you follow links; it also previews local text files. It is a text based
web reader with back/forward history and does not execute JavaScript or embed ForgeIQ's web app. The
voice button starts Windows voice typing in the chat composer; it uses the OS
speech feature. Unsupported actions show a
`NotImplemented` flash.

Current backend limits: most Python toolsets, connectors beyond the providers
listed above, OAuth login, cloud folder rename, cron and weekday schedules,
workflow connector/team nodes, and vector knowledge indexing are not ported.

The shared runtime is in `src/taiq/{agents,config,connectors,credentials,filestore,schema}`.
Desktop administration lives in `src/taiq/admin`; shared UI state and event
processing live in `src/taiq/ui/common`. Framework renderers live in
`src/taiq/ui/egui` and `src/taiq/ui/gpui`. UI extension descriptors are grouped
in `src/taiq_ext/plugins`. The default Cargo feature set excludes all three
desktop directories and the terminal TUI.
