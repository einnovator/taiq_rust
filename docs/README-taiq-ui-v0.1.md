# TaIQ desktop UI v0.1

## Goal and reference

Port the ForgeIQ web workspace shown in the screenshots to the TaIQ Rust GUI. The core workflow is to select an agent, have a conversation, and open or edit an artifact alongside it. Connector and agent configuration live in Settings. Use **TaIQ** as the desktop product name and treat **ForgeIQ** as the name shown in the reference screens. This handoff is based on four web screenshots and the SVG inspector brief. Confirm command semantics and supported formats against the current code before wiring them up.

## Workspace shell and navigation

| Surface | Web reference | Rust port |
| --- | --- | --- |
| Top bar | Menu, lightning logo and product name, “Knowledge Factory,” tagline, centered **Search files…** with **Ctrl+K**, theme, help, notifications and account. | Keep branding, file search and account controls in consistent positions. Let search contract as the window narrows. Put secondary actions in overflow when necessary. Keep the plan badge only where plan status is meaningful. |
| Navigation rail | Thin icon rail with active destination tinted blue. Screens show Chat, Files, Agents and Settings, alongside other workspace destinations. | Give each wired destination a tooltip, accessible name, selected state and keyboard path. Use the port’s route registry for additional destinations and verify icon-only routes before naming them. |
| Left sidebar | Changes content by destination. Agents lists agents, Files shows a tree, Settings lists categories. | Use one resizable sidebar with screen-specific content. Retain sidebar width, selection and tree expansion per workspace where relevant. |
| Main pane | Chat tabs above a chat header. Settings takes over the main region in the reference. | Preserve open conversations and draft state when navigating. Keep visible route and active-tab cues distinct. Provide tab overflow and show full titles on hover or focus. |
| Right pane | Independent artifact tab strip and a file-type-specific toolbar and content area. | Support resize, collapse and expand. Give chat and editor minimum usable widths. On narrow windows, allow the editor to take the main pane and provide a clear return to chat. Restore the split when the user reopens it. |

In the wide Files reference, the order is **navigation rail → file tree → chat → artifact editor**. In Agents and Settings, the center can fill the remaining width. Use the application layout as the visual reference. The browser tab strip, browser sidebar and OS taskbar sit outside the app.

## Chat and composer

The reference has a horizontal strip of conversations such as **Remote · Codix-2** and **Chat 5 · IQ**. Inside each tab, a smaller header shows the current agent’s avatar, name and role, with an agent dropdown and action icons. The empty conversation shows two centered suggestions, including a prompt about summarizing the open document. The bottom composer has an **Ask IQ** placeholder, microphone and blue send button. Below it, the web UI shows attachment and context icons, a labeled model row with provider and model, and several compact utility icons.

- **Input.** Pin the composer below the scrollable transcript. Grow it for multiline drafts up to a practical limit. Support paste, native text selection, IME composition, Enter to send and Shift+Enter for a newline, subject to the existing shortcut preference. Preserve drafts and attachments per chat tab. Keep the field usable when a send fails.
- **Context.** Show attached files, explicit artifact references and any other supported context as removable chips above the composer. An open tab can supply context through an explicit “Use open file” action or a visible active-file chip when the backend automatically includes it. Before sending, make it clear which artifact the request refers to. Use an open-document suggestion only when an eligible artifact is open.
- **Agent and model.** Keep the agent selector in the chat header and the effective model close to Send. Use a readable model label with provider and show the full identifier on demand. If the backend supports per-send model overrides, display the override and its scope. Resolve agent changes using the existing session behavior and expose when the action creates a new conversation.
- **Run states.** Add responsive sending, streaming, stop, retry and error states backed by the existing run APIs. Keep the transcript pinned to the newest message only while the user is following the stream. Preserve text selection, show a jump-to-latest affordance when scrolled back, and render code blocks with copy and horizontal overflow.
- **Action hierarchy.** Keep attachment, model and send easy to find. Put lesser-used actions in a labeled menu at small widths. Keep microphone and other utilities only where supported and give every icon a tooltip. Verify the precise action behind each web icon before porting it.

## Agents

The Agents sidebar shows compact avatar rows with an agent name and secondary role. Examples span **IQ · General**, **Max · Manager**, **Ada · Coding**, **Piper · Data Analysis**, **Morgan · Document Authoring**, **Nova · Orchestrator**, and **Codix-2 · Coding**. The sidebar header has add, folder-like, search and overflow icons. The selected row carries a blue-gray highlight.

Use reusable avatar/name/role rows, keyboard selection and search over names and roles. Pull names, avatars and roles from stored configuration. Provide an initials fallback for missing images. Make selection, opening a chat and managing the agent configuration clear actions. Keep the chat header and sidebar selection synchronized when a conversation is active. Put administration under **Settings → Agents**, using existing permissions and contracts for agent defaults, model, tools, skills, connectors and runtime where supported. Honor the app’s existing session reuse and run isolation settings wherever the backend exposes them.

## Files and artifacts

The Files sidebar displays a nested **Documents** tree with folder disclosure arrows, file-type icons and a selected row. Its toolbar shows create, upload, folder, plug, refresh, search and overflow icons. The top-bar file search is distinct from this local view.

- Open a file in the artifact pane through a stable workspace/file identifier. Keep duplicate filenames distinguishable via path in tooltips or the tab menu. Preserve expansion and focus after refresh when the items still exist.
- Support fast local filtering, with ancestor folders visible for matching descendants. Open shared search results in the same artifact pane. Show loading, empty folder, upload progress, error and retry states in place.
- Use native open dialogs and drag-and-drop for upload where supported. Distinguish opening a file from attaching it to chat. Provide a labeled **Attach to chat** action where appropriate. Wire the plug action after confirming whether it opens connector-backed files, connection management, or another workflow.
- Use editor tabs for documents in progress. Show dirty and saving states, keep view state per artifact, and prompt when closing a dirty tab without autosave. Keep permissions, save conflicts and external file changes visible.

## Editors and viewers

The SVG reference shows a file tab strip, controls for editing and inspection, a tool rail inside the editor, a dark grid around the canvas, and zoom, fit/expand and source controls. A narrow split clips the artwork at the displayed 100% zoom.

- Use a shared artifact host with per-format viewer/editor capabilities. Open the best registered view for the artifact type and expose **Edit**, **Source** and **Preview** only when supported. Confirm supported text, code, Markdown, PDF, document and media formats from the port’s available adapters.
- For SVG, provide Fit, zoom out/in, zoom percentage, pan, source view and expand. Fit the initial preview to the available viewport, then preserve explicit zoom and pan per artifact. Keep file-specific actions above the viewport and selection/drawing tools in their own rail. Editing and animation playback should follow the renderer’s supported features. Verify an animated SVG such as the one shown in the Files screen.
- Dock SVG inspectors inside the artifact pane. Keep its **Fill**, **Stroke paint** and **Stroke style** tabs and use the existing [SVG Fill and Stroke brief](README-svg-fill-stroke.md) for detailed paint controls. In a narrow split, collapse the inspector or use an in-editor drawer with a clear close action.
- Scope editor shortcuts to the focused pane. Undo in an editor should apply to that document. Escape should close a local popup or inspector first. Resize and preview changes should preserve selection and unsaved edits.

## Connectors and Settings

The Settings sidebar lists **Members, Connectors, Agents, Credentials, Tools, Skills, Models, Runtimes, Plugins**. The active category has a full-width highlight. The web connector screen shows a **Workspace / Global** scope selector, action toolbar, category dropdown, search, **All / Available** filter and results grouped by category. Visible Collaboration entries include **Confluence, Jira, Linear and Notion**, each with a logo, name, connection type and description. Their cards show **Global**, **Available**, **Auto-enabled**, info icons, **Test**, and overflow.

| Control | Desktop behavior |
| --- | --- |
| Scope | Keep Workspace / Global prominent. Show the effective scope of edits and fetch the corresponding data. Verify how the per-connector Global toggle relates to the page scope. |
| Filters | Keep category, query and availability visible above the list while it scrolls. Restore filters when returning to the same scope. Provide a clear empty result state. |
| Connector row | Compact the whitespace while keeping the logo, title, type, short description and row actions. At smaller widths, place controls beneath the description. Keep labels next to switches and put explanatory text in accessible tooltips or details. |
| Policy and health | Bind Global, Available and Auto-enabled independently to existing settings. Present connection health and credential status separately. Explain permission restrictions and show pending, success or failed changes in the row. |
| Test and configuration | Show progress and a useful result beside Test. Route credential changes through the existing credential flow. Keep secrets out of list summaries, logs and error messages. Name icon-only actions after verifying what each action does. |

Use the same Settings shell for the other categories, with per-page titles and predictable actions. Keep workspace and global configuration visibly scoped. Preserve the previous chat or editor state when closing Settings.

## Visual style and desktop conventions

- **Dark theme.** The reference uses a deep navy frame and panels, cool gray dividers, near-white text, muted labels and bright blue action accents. Active tabs and tree rows use a slate-blue fill. Use semantic tokens for background, surface, border, text, accent, selection, hover, focus and error. Distinguish selected, disabled and focused states through shape, text or stroke in addition to color.
- **Light theme.** The reference keeps the masthead dark over a pale workspace, white panels, soft gray borders and pale-blue selections. Apply the same component geometry and information hierarchy in both themes. Test connector logos, placeholders and disabled icons for legibility in both.
- **Density.** Use a compact icon rail, small-radius buttons, moderately rounded panes, aligned file rows and restrained borders. As starting desktop sizes, try a rail near 52 logical pixels and controls around 32–40 logical pixels high. Tune at OS display scaling and with long labels. Keep draggable splitters easy to grab and avoid pure icon cues for ambiguous commands.
- **Focus and navigation.** Make keyboard focus visible across the rail, tree, tabs, transcript, editor and settings. Bind Ctrl+K to file search on Windows/Linux and the platform-equivalent shortcut on macOS if supported. Provide keyboard tab switching and an action to move focus between panes. Keep transient menus and tooltips within the window.

## Rust implementation priorities

1. **Shell and layout.** Implement or reuse the navigation rail, resizable panes, tab strips and semantic theme. Verify wide and narrow states before adding page-specific controls.
2. **Agent-to-chat flow.** Wire agent selection, session tabs, composer, model and streaming through existing actions. Keep drafts, scroll state and tool output associated with their session IDs.
3. **Files beside chat.** Wire the file tree, quick search, artifact tabs and editor dispatch. Add explicit chat context from an open artifact and verify save/close behavior.
4. **Settings and connectors.** Port the Settings shell and connector list, keeping configuration scope and test state explicit. Apply the same page structure to the other settings categories.
5. **Editor fidelity.** Refine format-specific controls and SVG inspectors after the artifact and save flows work end to end.

Keep workspace, session, file and connector state keyed by their backend identifiers. Keep layout widths, selected tabs and drafts in UI state. Reuse current backend APIs and app actions. Run streaming, connector tests and file I/O asynchronously and route updates back to the target session, artifact or connector even if the user navigates while work is in progress.

## Acceptance walkthrough

- Open **Agents**, choose **IQ**, send a multiline message with an attachment, switch tabs mid-stream and return to the correct transcript. Check that an unsent draft in another tab survives.
- Open **Files**, navigate an expanded tree, then find the same file with **Ctrl+K**. Open an animated SVG beside chat, fit it to the pane, resize the split and use the open file as visible chat context.
- Edit and save a supported artifact. Switch routes and return to the existing editor tab. Close a dirty tab and verify the save or discard flow.
- Open **Settings → Connectors**. Switch scopes, search and filter, then test one connector. Check that each policy toggle updates the intended scope and that an error remains associated with the correct row.
- Repeat the core flow with keyboard only, in light and dark themes, and at a narrow window size with display scaling. Verify that Send, file opening, connector testing and return from a full-width editor remain reachable.
