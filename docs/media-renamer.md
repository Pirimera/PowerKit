# MediaRenamer design

MediaRenamer is implemented. Unit and integration tests on native ARM64 cover
the domain, the WIC/BMFF probe with synthetic JPEG/MP4/MOV fixtures, the
scan, the journalled commit and undo, and a full session flow. Physical
acceptance (HEIC and Live Photo pairs, camera RAW, missing codecs, OneDrive
placeholders and Explorer multi-select) is still open. This document records
the product decisions and how the implementation meets them.

MediaRenamer renames photos and videos in place from their capture metadata.
It is a [workspace](ui-structure.md): files are added, previewed and renamed on
the tool page; lasting defaults live in its Settings drawer.

## Scope

In the first release:

- Rename files in their current folder from a token template. Moving files into
  date folders, copying to another destination and import workflows are not
  planned for the first release.
- Add files from the Explorer context menu, drag and drop onto the page, or the
  page's file and folder pickers.
- Read capture time, sub-second time, time-zone offset, camera make and model.
- Rename Live Photo pairs and sidecars together with their primary file.
- Require a full preview before any write; never overwrite an existing file.
- Keep a persistent, bounded History so a batch can be undone after PowerKit
  restarts.

Not planned for the first release: metadata writing (date shifting stored in
the file), regex find/replace, folder moves, duplicate detection, cloud
libraries, automation (CLI/IPC) and watch folders.

## Formats and metadata sources

| Group | Extensions | Reader | Capture time | Sub-second / offset | Camera |
| --- | --- | --- | --- | --- | --- |
| JPEG | `.jpg` `.jpeg` `.jpe` | WIC metadata query reader | EXIF `DateTimeOriginal` (36867), then `DateTimeDigitized` (36868) | `SubSecTimeOriginal` (37521), `OffsetTimeOriginal` (36881) | IFD0 `Make` / `Model` |
| HEIF | `.heic` `.heif` `.hif` | WIC (HEIF Image Extensions) | EXIF as above | as above | as above |
| RAW | `.dng` `.cr2` `.cr3` `.nef` `.arw` (plus other formats the Raw Image Extension decodes) | WIC (Raw Image Extension) | EXIF as above | as above | as above |
| PNG / WebP | `.png` `.webp` | WIC | EXIF chunk when present | as above | as above |
| ISO BMFF video | `.mp4` `.mov` `.m4v` `.3gp` | PowerKit box reader, Property System as fallback | Apple `com.apple.quicktime.creationdate` (local time with offset), then `mvhd` creation time (UTC; minus the duration for Android recordings) | offset only from the Apple key | Apple `make` / `model` keys |
| AVI | `.avi` | Property System (`System.Media.DateEncoded`) only; the `IDIT` chunk is not read | best effort | none | none |

Notes:

- Capture time comes from the file content, never from the Windows Property
  System's `System.Photo.DateTaken`, because that property is already converted
  to UTC using the machine's time zone and loses the original offset.
- When EXIF has neither original nor digitized time, WIC images fall back to the
  XMP `exif:DateTimeOriginal` and then `xmp:CreateDate`. That value is reported as
  the EXIF original.
- PNG and WebP rarely carry EXIF, and many AVI files have no usable date. Those
  files usually fall into the missing-date path below; the preview shows that
  before anything is written.
- WIC needs the HEIF, Raw Image and WebP Image Extensions from the Store for
  those formats. If a codec is missing, the row reads "Bu biçim için Windows
  uzantısı gerekli" (with the extension name) instead of a generic error.
  PowerKit does not install codecs.
- The ISO BMFF reader is a small pure Rust parser over `ftyp`, `moov/mvhd`
  (creation time, timescale and duration) and `moov/meta` (`keys` + `ilst`). It reads boxes only, follows 64-bit sizes,
  caps depth and box counts, and treats malformed input as "metadata
  unreadable". It needs no new dependency.
- The format list is a closed allow-list in the domain module. Other files in a
  dropped folder are skipped and counted, not listed as errors.

Observed on the ARM64 development machine: Windows registers the
image property handler for every image extension above and the Media
Foundation MPEG-4 handler for `.mp4` `.mov` `.m4v` `.3gp` (AVI has its own MF
handler). HEIF, Raw Image, WebP, HEVC and AV1 extensions are all installed, so
the missing-codec path has not been observed yet. Exact WIC query paths for
HEIF, RAW and WebP still need a read-only probe with synthetic fixtures.

## Time interpretation

The filename always shows the local wall-clock time at capture:

1. EXIF time with `OffsetTimeOriginal`: use the wall-clock value as recorded.
   The offset is kept for display and for comparing files from different
   devices.
2. EXIF time without an offset: use the wall-clock value as recorded. EXIF
   already stores local camera time, so it is not converted.
3. Apple QuickTime `creationdate`: use its local value and offset.
4. `mvhd` creation time (UTC): convert to local time with the Windows time-zone
   rules for that date (`SystemTimeToTzSpecificLocalTimeEx` with dynamic DST),
   not today's offset.
5. An `mvhd` value of zero or before 1970 counts as missing.
6. Android recordings: the Android muxer writes `mvhd` when the recording
   stops, so its creation time is the end of the clip. When `moov/meta` carries
   the `com.android.version` key and there is no Apple `creationdate`, the
   `mvhd` duration (whole seconds) is subtracted to get the start. The source
   then reads "QuickTime (UTC'den yerel saate, kayıt süresi çıkarıldı)". Files
   without that key are not adjusted, because cameras stamp the start. A
   missing, zero or unknown duration leaves the time as stored.

Limitation: a video without the Apple key gets the computer's current time zone
rules, which is wrong for clips recorded while travelling. The Details drawer
shows the source ("QuickTime UTC, bilgisayarın saat dilimine çevrildi") so the
user can see why.

Per-batch time shift (camera clock wrong) is not in the first release.

## Missing dates

A file with no usable capture time is listed with the state "Tarih yok" and is
excluded from the rename by default. A command-bar toggle, "Tarih yoksa
değiştirme tarihini kullan", includes those rows using the file's last-write
time for this run only. Each affected row then shows the source badge
"Değiştirme tarihi". The toggle's default lives in the Settings drawer and is
off. Creation time is never used, because copying a file resets it.

## Template

Templates are token strings. Literal text is allowed between tokens.

| Token | Value |
| --- | --- |
| `{yyyy}` `{yy}` | Year |
| `{MM}` `{dd}` | Month, day (two digits) |
| `{HH}` `{mm}` `{ss}` | Hour (24 h), minute, second (two digits) |
| `{fff}` | Milliseconds from sub-second data; empty when absent |
| `{make}` `{model}` | Camera make / model |
| `{original}` | Original file name without extension |
| `{counter}` `{counter:N}` | Position in the preview order, zero-padded to N digits (default 3) |

- Default template: `{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}`, matching the roadmap's
  `YYYY-MM-DD_HH-mm-ss` default.
- Presets in the command bar: the default, `{yyyy}{MM}{dd}_{HH}{mm}{ss}`,
  `{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}_{model}` and
  `{yyyy}-{MM}-{dd}_{original}`.
- Token chips under the template box append a token to the end of the template.
  Caret-position insertion is later work ([roadmap](roadmap.md)). Chip labels are
  the raw token names, so a template reads the same in every language.
- An unknown or unbalanced token is an inline validation error and disables
  Apply. Token names are case-sensitive (`{MM}` month, `{mm}` minute).
- A token with no value (no camera model, no sub-seconds) renders empty, and
  the separator directly before it is dropped so names never end in `_`.
- The file extension is preserved; a Settings drawer option lowercases it
  (`.JPG` → `.jpg`), off by default.
- Sanitizing: characters Windows rejects become `_`; trailing dots and spaces
  are trimmed; reserved device names (`CON`, `NUL`, `COM1`…) get a trailing
  `_`; a name component is capped at 255 UTF-16 units including the extension.
  An empty result becomes `media`.

## Collisions

Collisions are resolved deterministically in preview order (folder, then
capture time, then original name):

1. If the rendered name is already taken, by a file on disk or by an earlier
   row in the batch, and the template does not already contain `{fff}`, append
   `_` plus the sub-second value (for example `2026-05-01_14-03-22_457`).
2. If it is still taken, or there is no sub-second value, append `_1`, `_2`…
   up to `_9999`; beyond that the row fails as "Ad çakışması çözülemedi".
3. A file whose new name equals its current name is "Değişmeyecek" and is not
   touched. A case-only change (`img.jpg` → `IMG.jpg`) is a real rename.
4. Names freed by other rows in the same batch can be reused (A → B while
   B → C); the commit handles ordering, see below.

Each row shows a collision badge when steps 1 or 2 changed its name.

## Live Photos and sidecars

Files in the same folder with the same stem (case-insensitive) form a group:

- Primary selection: among members with a capture time, JPEG and HEIC beat RAW,
  and RAW beats video. A member without a time never beats one with a time.
- Companions: the paired video (`.mov`, `.mp4`), `.aae`, and XMP sidecars in
  both forms (`IMG_1234.xmp` and `IMG_1234.CR3.xmp`).
- Every member gets the primary's new stem and keeps its own extension
  (`.CR3.xmp` stays a double extension). A collision suffix chosen for the
  primary applies to the whole group.
- The table shows one row per group with the member count; the Details drawer
  lists every member. Excluding a group excludes every member.
- Companions are added automatically even when the user dropped only the
  primary, and are shown as such. `.aae` and `.xmp` files are never renamed on
  their own.

## Workspace UI

Follows the workspace skeleton in [Tool UI structure](ui-structure.md):

- **Header:** title "MediaRenamer" with a status pill (busy spinner or ready
  count), live summary (items, files, ready and problem counts), command bar:
  "Dosya ekle…", "Klasör ekle…", a vertical separator, History and the gear
  Settings button as the rightmost command.
- **Template bar:** template box with preset and token menus (a chosen token is
  appended to the end of the template), a reset to the saved default, a live
  example (`current → new`, or a synthetic sample before files are added) or
  the inline template error, and a two-column row of "Alt klasörleri dahil et"
  (applies to the next folder added) and "Tarih yoksa değiştirme tarihini
  kullan" toggles.
- **Filter and action bar:** two-row toolbar matching PowerStore — search box
  with clear button and state dropdown (showing counts for all, ready,
  unchanged, no date, collision and problem) on the first row; one-click status
  filter pills (`Tümü`, `Hazır`, `Sorunlar` plus the active secondary filter) on
  the left and "Yeniden adlandır" (Accent, disabled until probing is complete
  and at least one ready row is selected) plus "Temizle" on the right of the
  second row.
- **Table:** select checkbox (selected rows are renamed), 32 px media-kind icon
  tile, current name with companion count pill and parent folder caption,
  accented `→ new` name, capture time with source caption (EXIF, QuickTime,
  Değiştirme tarihi), and semantic state/collision pills (or an inline progress
  ring while probing). Selected ready rows carry a left accent bar. A header
  checkbox selects all visible rows. The table is virtualized, so every visible
  row is listed and only rows on screen are realized. Editing the template
  re-renders new names without re-reading metadata. When a filter matches no
  rows, an inline empty state offers "Filtreyi temizle".
- **Empty state:** drop-zone card with icon badge, hint text and the two add
  buttons; the page has `allow_drop` with the windows-reactor `on_drop`
  storage-item payload.
- **Drawers:** 460 px trailing-edge overlay drawers shared with PowerStore
  (dimming scrim, rounded leading corners, 36 px icon-badged headers): Details
  (one file or group: status pills, capture time, offset, source and camera
  facts, hairline-grouped member list, and every metadata value of each media
  member), History (sticky outcome-count summary + Clear bar, left-accented
  batch cards with sample `from → to` boxes and Undo) and Settings (default
  template card with dirty-aware Save button, hairline-grouped default toggles
  and File Explorer notice).
- **Details metadata list:** opening Details starts a worker that reads each
  media member of the group once, only while the drawer is open (closing or
  switching drawers stops it; a refresh that changes files reads again). Images
  are enumerated through WIC: the frame and container query readers, with the
  known blocks (`/app1/ifd`, `…/exif`, `…/gps`, `…/interop`, `…/thumb`, `/xmp`,
  and the same under `/ifd` for TIFF-style containers) asked for by path,
  because WIC flattens sub-IFDs when a parent block is enumerated and tags
  would lose their block. Other blocks (JFIF, PNG text chunks and so on) are
  listed under their own name; JPEG quantization tables are left out. Videos
  and AVI list their Windows Property System values. Values come out as stored:
  tag names are the standard English EXIF names (unknown ids read `Tag N`),
  enumerated numbers (for example `Flash`) are not translated to words, rationals show as
  `1/250 (0.004)` and binary data (maker notes) as a short hex run with its
  size. Sidecars and online-only files are not read, because reading a
  placeholder downloads it. One file is capped at 1,000 tags, 512 characters
  per text value and 32 list elements. The list is selectable text.
- **Progress:** scanning and probing stream rows into the table with a
  cancellable progress strip; applying shows per-file progress and can be
  cancelled between files. When a preview is stale, the strip offers
  **Yenile** (refresh).

Labels sit left of toggles, as everywhere in PowerKit. All strings go through
`crates/core/src/locale.rs` in Turkish, English and Arabic; drawers open from the trailing
edge so they mirror in right-to-left layout.

Navigation and tray: MediaRenamer joins the rail after PowerStore (workspaces
before utilities). The tray gets a MediaRenamer submenu with a disabled status
row (idle, scanning, "N dosya hazır", applying) and "MediaRenamer'ı aç". General
shows one card with status and an Open action.

## Input paths

- **Explorer:** "PowerKit ile yeniden adlandır" on supported media files and on
  folders, in both the Windows 11 compact menu and the classic menu. It reuses
  the ImageResizer pattern: a second COM class in `powerkit_shell.dll`, the
  per-user sparse package for the compact menu, a classic
  `SystemFileAssociations` (and `Directory`) verb with
  `MultiSelectModel=Player`, and
  `powerkit.exe --rename <paths>` / `--rename-list <file>` forwarded to the
  running instance as `WM_COPYDATA`. Explorer input replaces the current
  workspace list after a confirmation if the list has unapplied rows. Without
  the shell DLL the classic verb starts one process per item, so handoffs that
  arrive within two seconds of the previous one extend that selection instead
  of replacing it. The MSIX manifest declares the same command (files and
  folders) through the package's file-explorer context-menu extension; packaged
  builds skip the runtime registration and have no classic verb.
- **Drag and drop** onto the page adds to the current list.
- **Pickers:** "Dosya ekle…" (multi-select, filtered to supported extensions)
  and "Klasör ekle…".
- **Folders:** only the selected folder by default; subfolders when the toggle
  is on. Recursion does not follow junctions or symbolic links, skips hidden
  and system files, and stops at 20,000 files per batch with a visible notice.
- Duplicate paths (case-insensitive, after resolving `\\?\` and short names)
  are added once.
- Paths of 248 UTF-16 units or more use the `\\?\` form (`\\?\UNC\` for network
  paths) for renames, WIC and Property System reads and history writes, so
  renames work beyond `MAX_PATH` without the long-path manifest opt-in.
- Cloud placeholders (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`, for example
  OneDrive online-only files) are not read, because reading them downloads the
  file. They are listed as "Yalnızca çevrimiçi" and excluded.

## Apply and recovery

The commit runs on a worker thread and never touches file contents:

1. **Revalidate:** for each selected row, the source still exists with the size
   and last-write time recorded at probe time, and the target is still free.
   Any mismatch marks the preview stale and stops before the first rename.
   **Yenile** re-checks every listed file: changed files are read again, missing
   files leave the list, folder contents refresh the collision checks, and the
   preview is planned again. Nothing is renamed by a refresh. Online-only
   placeholders are checked by size and time but never read, because reading
   them downloads them.
2. **Journal first:** write the batch to the History file with state `pending`
   (temporary file + rename), listing every source, target, size and
   last-write time.
3. **Rename:** `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`, so an
   unexpected existing target fails instead of being overwritten. Chains and
   swaps go through a unique temporary name in the same folder first. Each
   group is renamed as a unit; if a member fails, the group's earlier members
   are moved back.
4. **Partial failure:** a failed row or group (locked, read-only folder,
   access denied) is reported and the batch continues. The History entry
   records only what was applied.
5. **Finish:** rewrite the entry with state `done` and the applied list.

On launch, a `pending` entry (PowerKit was killed mid-batch) is checked against
disk and shown in History as "Yarım kaldı" with an Undo for the renames that
did happen.

**Undo** reverses a batch newest-first. A file is moved back only if the target
still exists with the recorded size and last-write time and the original name
is free; otherwise that file is skipped and listed. Undo is itself recorded in
History. Undo never deletes or overwrites.

## Persistence

Settings through the existing settings writer:

| Key | Meaning |
| --- | --- |
| `rename_template` | Default template (validated; invalid values fall back to the default) |
| `rename_recurse` | Default for "Alt klasörleri dahil et" |
| `rename_mtime_fallback` | Default for the missing-date toggle |
| `rename_lowercase_ext` | Lowercase extensions |

History lives in `media-renamer-history.txt` beside `settings.txt` (header
`powerkit-rename-history=1`, tab-escaped lines, temporary file + rename on
every change). It keeps the 50 most recent batches and at most 20,000 entries
per batch, and stays within 64 MiB: writes drop the oldest batches to fit, and
a single batch that alone exceeds the budget is refused before any rename. A
history file that exists but cannot be read (too large, or not a PowerKit
history) is never overwritten. Renames, undo and clearing stay off until the
file is moved or deleted and the app restarts. The workspace list, preview and
selection are not persisted; a restart starts with an empty workspace.

## Threading and safety

- Scanning, WIC/Property System reads and the commit run on worker threads.
  COM is initialized per worker; WIC factories and property stores stay on the
  thread that created them. Results return to the UI thread as messages.
- Scans are cancellable: Clear cancels the running scan. A new add made during
  a scan queues behind it and starts when that scan finishes.
- MediaRenamer never runs elevated and never edits metadata or file contents.
- A running batch does not stop or restart an AlwaysOn session; exit waits for
  the current file, records the journal and stops.

## Module layout

- `crates/core/src/media_renamer.rs`: the pure domain root. It declares the submodules
  below and holds the allow-list kinds, `MAX_BATCH` and `Preferences`. Unit
  tests run without Windows.
- `crates/core/src/media_renamer/template.rs`: token grammar, parsing, rendering and
  sanitizing.
- `crates/core/src/media_renamer/time.rs`: capture times, offsets, time sources and the
  time-zone interpretation inputs.
- `crates/core/src/media_renamer/bmff.rs`: pure ISO BMFF box reader over byte slices.
- `crates/core/src/media_renamer/exif.rs`: names, sections and display values for the
  Details drawer's metadata list (EXIF, GPS, Interop tag tables, rational and
  byte formatting, caps).
- `crates/core/src/media_renamer/plan.rs`: groups, probe results, rows, collision planning
  and the summary.
- `crates/core/src/media_renamer/history.rs`: journal format, byte and entry budgets, and
  undo planning.
- `crates/core/src/media_extensions.rs`: supported extensions, shared by the app and the
  shell DLL.
- `crates/app/src/controller/media_renamer/`: `mod.rs` (the `Session` on the main
  thread: state, rows, apply, undo, refresh and polling), `job.rs` (worker
  threads and the wake), `commit.rs` (revalidation, journal first, temporary
  names for swaps, group rollback, undo, history load) and `view.rs` (page
  snapshots). It reaches files only through the `MediaFiles` port; its tests
  run over an in-memory folder.
- `crates/platform/windows/src/media_renamer/`: `mod.rs` (the port on
  Windows), `scan.rs` (folder walk, placeholders, duplicates, refresh
  checks), `probe.rs` (WIC + Property System), `commit.rs` (`MoveFileExW`
  without replace, fingerprints, the atomic history file write), `pick.rs`
  (file and folder pickers), `handoff.rs` (`--rename` arguments, classic
  verbs) and `tests.rs` (session flows on synthetic fixtures).
- `crates/ui/reactor/src/window/rename_page.rs`, `rename_drawer.rs`: the page and
  its drawers. Shared Fluent helpers live in `window/view_helpers.rs`.
- `crates/core/src/locale/media_renamer.rs`: Turkish, English and Arabic strings.
- `apps/shell-ext/src/lib.rs`: second command class for the rename verb.

## Testing

All fixtures are synthetic; user media is never read during development.

- Unit tests: template grammar and errors, separator dropping, sanitizing,
  reserved names, collision order (sub-second then counter), chain and swap
  planning, grouping rules, BMFF parsing of hand-built boxes (including
  truncated, oversized and 64-bit boxes), time-zone conversion inputs and
  History round-trips with corrupted lines.
- Windows integration tests in a temporary directory: generate JPEG/PNG files
  with EXIF through WIC, write minimal MP4/MOV byte fixtures, rename, kill-safe
  journal recovery, undo after external modification, read-only file failure.
- Physical: HEIC and Live Photo pairs from a test device, RAW from a camera,
  missing-codec behaviour on a machine without the extensions, OneDrive
  online-only files and Explorer multi-select. Record native ARM64, emulated
  x64 and physical evidence separately.

## Product decisions

- Rename in place only; folder moves and copies are later.
- Input from Explorer, drag and drop and pickers; Explorer opens the workspace.
- Token templates with presets and chips.
- Local capture time: EXIF as recorded, video UTC converted to local time.
- Missing dates are flagged and excluded; a per-run toggle uses last-write time.
- Collisions: sub-second first, then `_N`.
- Live Photo pairs and sidecars are renamed as a group.
- History is persistent and bounded, with undo across restarts.
- Formats: JPEG, HEIF, MP4, MOV, plus PNG, WebP, RAW, M4V, 3GP and AVI.
- Folders: selected folder only by default, with an opt-in subfolder toggle.

## Open questions

- Exact WIC metadata query paths for HEIF, RAW (per vendor) and WebP: the
  JPEG EXIF and MP4/MOV paths are confirmed by tests on synthetic fixtures,
  the others still need physical files.
- Whether Microsoft's MPEG-4 property handler exposes the Apple creation date
  on all Windows builds, or the BMFF reader must be the only source.
- Partial-failure policy is "continue and report"; revisit if users expect a
  whole-batch rollback.
