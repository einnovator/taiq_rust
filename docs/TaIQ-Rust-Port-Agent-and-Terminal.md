# TaIQ desktop port — Agent selection, agent settings, and terminal

Continue the existing standalone Rust port. Use the attached web screenshots for layout and the existing desktop README for workspace conventions. Keep the port's current UI framework, theme tokens, actions, and backend interfaces. The desktop product name is **TaIQ**. The screenshots show **ForgeIQ**.

## Agent selector and composer controls

Place the agent selector at the left of the active chat header, directly beneath the conversation tabs. Use a compact trigger with a circular avatar, agent name, muted role, and down chevron, as in **IQ · General**. The conversation tabs above it identify existing conversations, such as **Remote · Codix-2** and **Chat 5 · IQ**. Keep switching tabs and choosing an agent as separate actions.

The screenshots show the agent trigger closed. For the native dropdown, reuse the existing agent rows and catalog. Show avatar, name, and role, highlight the current agent, and support filtering by name or role, arrow keys, Enter, and Escape. Use stable agent IDs and an avatar fallback. On selection, use the existing session reuse or creation rules, then update the header, composer placeholder (for example **Ask IQ**), and effective model together. Preserve drafts, attachments, and streaming output under their existing conversation identities. Confirm the exact switching and in-flight behavior against the web source or backend contract.

| Composer control | Reference and port behavior |
| --- | --- |
| **Model** | The labeled row below the prompt displays a provider icon, effective model identifier, capability icons, and a chevron. Open a wide, upward-anchored popup with a focused search field, scrollable results, and a blue selected row. Each result shows its provider, model ID, and available capability icons. Use live model metadata and expose the effective override scope from the existing app. |
| **Reasoning settings** | The gauge icon opens a small popover above the toolbar. It shows **Reasoning**, a slider, and the selected value, shown as **medium**. Use supported values for the effective model and follow the current override rules when switching models. |
| **Terminal** | The terminal glyph at the far right of the composer toolbar opens the docked terminal. Keep the button reachable in a narrow chat pane. |

## Agent settings

Open the selected agent's settings from the gear in the chat header. Keep the workspace rail, Files sidebar, conversation tabs, and agent header visible. The settings body occupies the chat region and uses a narrow local navigation column with icons and a blue active row, plus a wider scrollable content area. Put the section toolbar above the content. The reference shows Save and a second gear at the right, with add actions in some sections.

Use this navigation order: **Agent, Prompts, Connectors, Tools, Skills, Schedules, UI, Memory, Channels, Models, Runtime**. The four pages below are visible in the attachments. Use the existing configuration contracts for the other sections and verify their layouts and toolbar actions in the web code.

| Page | Controls shown |
| --- | --- |
| **Tools** | Category dropdown defaulting to **All**, **Search tools**, and a scrollable list. Each row shows an icon, name, category label, and description. Right-align separate **Enabled** and **Autoapprove** switches. Use the stored values and respect existing tool policy and confirmation rules. |
| **Skills** | **All categories**, **Search skills**, **All / Enabled** radio filter, and a plus action. Group entries by source, such as **Local**. Cards display name, category and description, and an **Enabled** switch. |
| **UI** | **UI Enabled** switch, **Renderer** dropdown shown as **Auto**, multiline **Workflow Instructions** and **UI Description** fields, and collapsed **Advanced options**. Workflow Instructions guide when to emit an A2UI surface or plain text. UI Description guides component choice and layout within that surface. |
| **Memory** | Two side-by-side panels, **Personal Memory** and **Shared Memory**, with separate backend selectors. The examples are **LOCAL** and **SAME BACKEND**. Each panel has independent **Auto Recall** and **Auto Remember** switches. **Memory Files** sits below. Stack the panels at narrow widths. |

Bind edits and save results to the agent that owns them, even when the user changes tabs during a request. Check which controls save immediately and which use the toolbar Save action. Reuse the existing dirty-state and error handling. Keep this agent-specific editor distinct from workspace/global Settings and from the composer’s effective model selection.

## Docked terminal

Open the terminal **inside the chat pane, between the transcript and the composer**. The transcript gives up height while the composer stays visible below. The Files sidebar remains full height. Use a horizontal drag splitter immediately above the terminal with usable minimum heights. The reference shows a dark terminal surface within the light workspace.

- **Header.** Show the terminal icon, **Terminal**, and the effective working directory. The screenshot shows a Windows workspace path. Resolve the path through the configured workspace and execution context. Right-side controls show a square, a circled down arrow, and a gear. Confirm their exact handlers before assigning labels or wiring them.
- **Output.** Provide a scrollable monospace output region. The example shows **Terminal ready** followed by a PowerShell prompt. Use the actual shell and session status. Route streamed output and errors to the owning terminal session even while the user is on another chat tab.
- **Command entry.** The web view shows a separate bottom input with a prompt glyph, **Enter a command**, and a return icon. Send Enter to the terminal while its input has focus and keep command history and editing behavior aligned with the existing execution API. Preserve normal chat input behavior when focus returns to the composer.
- **Lifecycle.** Check the existing behavior for local versus remote targets, shell choice, working directory, stop or interrupt, and whether hiding or changing tabs retains the session. Reuse the existing terminal transport when available. Match the separate command field in the reference and use a native PTY where the supported execution contract requires an interactive terminal.

## Acceptance walkthrough

1. Open two conversations with different agents. Change the active agent and verify the session follows the existing reuse rules. Check the avatar, role, prompt placeholder, and model after switching tabs.
2. Filter the model list and choose a model by keyboard. Check the effective model and available reasoning values, then close each popup with Escape and verify focus return.
3. Open an agent's settings from the chat header. Filter Tools and Skills, change a setting for that agent, save through the existing flow, and reopen to verify it. Check that another agent's settings are unaffected. Verify independent Memory switches and the two UI instruction fields.
4. Open and resize the terminal. Run a harmless command such as printing the working directory. Confirm the displayed directory and output match the selected target, test the configured stop action with a long-running command, and switch chat tabs while output is arriving. Confirm that terminal input and the chat draft stay independent.

## Reference screenshots

| Surface | Attachment |
| --- | --- |
| Tools | image(20261125-114648).png |
| Skills | image(20261125-114654).png |
| UI | image(20261125-114704).png |
| Memory | image(20261125-114716).png |
| Model picker | image(20261125-114821).png |
| Reasoning popover | image(20261125-114837).png |
| Terminal | image(20261125-114913).png |
