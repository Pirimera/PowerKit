# Tool UI structure

Every PowerKit tool page follows one of two patterns so users learn where work
happens and where settings live without reading labels. The pattern is a layout
contract; it is not shown to users as a category name.

## Patterns

| Pattern | Work happens in | Persistent settings live in | Tools |
| --- | --- | --- | --- |
| Utility | Tray, hotkey, Explorer verb, overlay or a task window | The tool page itself | AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer, PowerMouse |
| Workspace | The tool page (table, preview, queue) | A right-side Settings drawer | PowerStore, MediaRenamer |

A tool that needs a one-off task flow (choose files, preview, run) stays a
utility when the flow is short and launched from outside PowerKit. Its task flow
opens in its own window (ImageResizer's resize dialog), not inside the page.
ImageResizer's page leads with Explorer instructions; an in-page file picker is
only a secondary fallback, never an Accent primary action. A tool whose main
value is browsing or editing a collection is a workspace.

## Where a setting goes

1. It changes only the current run: put it in the task flow (resize dialog,
   workspace command bar). Example: the size preset for this batch, a rename
   template for this preview.
2. It is the tool's lasting default: put it on the utility page or in the
   workspace Settings drawer.
3. It affects PowerKit as a whole (language, theme, acrylic tint, startup,
   tray, PowerKit updates): put it on the Settings page at the rail foot. Never
   put tool-specific settings there.

A task window may remember its last values (ImageResizer does, like PowerToys).
The utility page then says so beside the shared options, so users know the two
surfaces edit the same defaults.

## Utility page skeleton

1. Page title and one-line lead (shared `page_view` header).
2. A launch or status card first: what the tool is doing now and how to start
   it (hotkey, tray, Explorer verb), with the primary action button.
3. Grouped default settings under `settings_ui::section` headings.

## Workspace page skeleton

1. Header with title, live summary and a command bar. Primary actions sit on the
   left; the gear Settings button is always the rightmost command.
2. The workspace itself: one table or preview with a sticky filter/action bar.
3. Right-side overlay drawers: item details, History and Settings. Drawers open
   from the trailing edge, so they mirror under right-to-left layout.

Batch file edits (MediaRenamer) follow this order in the workspace: add files,
preview old name to new name with collision state, apply, then recover from the
History drawer.

## Terminology

| Term (TR / EN / AR) | Use for |
| --- | --- |
| Araç / Tool / أداة | Any PowerKit module in user text; do not say "mini-app" |
| Ayarlar / Settings / الإعدادات | Any settings surface; headers name the scope, such as "PowerStore ayarları" |
| Geçmiş / History / السجل | Completed operations and recovery |
| Ayrıntılar / Details / التفاصيل | One item's information drawer |

Do not label a settings surface "Manage" (Yönet): users cannot tell whether it
manages items or preferences. Use a content noun when a drawer lists content
rather than preferences. Buttons that open a window or picker end with an
ellipsis ("Resimleri seç…").

## Navigation, home and tray

The rail keeps Home (General page identity) first, then workspaces, then
utilities, with Settings at the foot. The tray lists one entry per tool, each
with its rail icon: a tool with several commands is a submenu (utilities expose
checks and radio items for their state; workspaces expose status and an "Open"
command), and a tool with a single command is that command at the root.
Settings chooses which tools the tray lists and their order. Home opens with a PowerToys-style welcome
hero, a grid of tool tiles with enable toggles, application info and the latest
changelog. Disabling a tool removes it from the rail and tray and stops it
system-wide (`module_*` settings keys; PowerMouse uses `mouse_enabled`).
