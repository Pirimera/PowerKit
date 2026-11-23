# MediaRenamer

[User guide](README.md) · [Türkçe](tr/media-renamer.md)

MediaRenamer renames photos and videos by the date and time they were taken,
for example `IMG_4821.JPG` becomes `2026-05-01_14-03-22.JPG`. Files stay in
their folder. Their contents and metadata are never changed, no file is ever
overwritten, and every rename can be undone.

## Rename files

1. Add files. Any of these works:
   - Drag photos, videos or folders onto the Media Renamer page.
   - Choose **Add files** or **Add folder** on the page.
   - In File Explorer, right-click files or folders and choose **Rename with
     MediaRenamer**.
2. Check the list. Each row shows the current name, the new name, when the
   file was taken and a status.
3. Change the **Name template** if you want different names. The new names
   update as you type.
4. Untick the rows you do not want to rename.
5. Choose **Rename**.

Nothing changes on disk before step 5.

A folder adds only the files directly inside it. Turn on **Include
subfolders** before adding the folder to take everything below it too. Hidden
and system files are skipped, and one list holds at most 20,000 files.

## Name template

A template is text with tokens in braces. Text outside braces is used as it
is. Token names are case-sensitive: `{MM}` is the month, `{mm}` the minute.

| Token | Value |
| --- | --- |
| `{yyyy}`, `{yy}` | Year, four or two digits |
| `{MM}`, `{dd}` | Month, day |
| `{HH}`, `{mm}`, `{ss}` | Hour (24-hour), minute, second |
| `{fff}` | Milliseconds, when the file records them |
| `{make}`, `{model}` | Camera make and model |
| `{original}` | The current file name, without its extension |
| `{counter}`, `{counter:5}` | Position in the list, with 3 digits or the number of digits you give |

The default is `{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}`. **Presets** offers a few
common templates, and **Insert token** adds a token to the end of the
template. **Reset to default** returns to the saved default.

- A token that has no value for a file, such as the camera model of a
  screenshot, is left out together with the separator before it.
- The file extension is kept.
- Characters that Windows does not allow in a file name become `_`.
- A template with an unknown token or unmatched braces shows an error and
  cannot be applied.

## Statuses

| Status | Meaning |
| --- | --- |
| Ready | The file will be renamed |
| Unchanged | The file already has the new name |
| No date | The file records no capture time; it is left out |
| Collision resolved | Another file had the same new name, so a suffix was added |
| Online-only | A cloud file that is not on this computer. It is not read, because reading would download it |
| Unreadable | The metadata could not be read |
| Needs … | A Windows extension is missing for this format; see [Supported files](#supported-files) |
| Name collision unresolved | No free name was found |

**Files without a date:** turn on **Use the modified time when there is no
date** to rename them by the time the file was last modified. A copied or
edited file can have a modified time that differs from when it was taken, so
check those rows.

**Two files with the same new name:** the second gets the milliseconds added
when the file records them (`2026-05-01_14-03-22_457`), otherwise `_1`, `_2`
and so on.

The search box and the status filters narrow the list. A row's **Details**
show where the time came from and all metadata of the file.

## Live Photos and sidecar files

Files in the same folder that share a name belong together: a photo and its
Live Photo video, a RAW file and its JPEG, and `.aae` and `.xmp` sidecar
files. They appear as one row, get the same new name and keep their own
extensions. Companions are added even when you added only the photo.

## Undo

**History** lists every rename batch. **Undo** gives the files of a batch
their old names back. History is kept when PowerKit restarts, so a batch can
be undone later.

- A file is moved back only if it has not changed since and its old name is
  still free. Other files are skipped and listed.
- Undo never deletes or overwrites a file.
- If PowerKit was closed in the middle of a batch, History shows that batch as
  "Interrupted, recovered" and can undo the part that was done.
- History keeps the 50 most recent batches.

If a file changes or disappears between the preview and **Rename**, nothing is
renamed. Choose **Refresh** to check the files again.

## Settings

The gear button opens the settings.

| Setting | What it does |
| --- | --- |
| Default template | The template the page starts with. **Make the current template the default** saves the one in use |
| Include subfolders when adding a folder | The starting state of that switch |
| Use the modified time when there is no date | The starting state of that switch |
| Lowercase extensions | Writes `.jpg` instead of `.JPG`. Off by default |

## Supported files

| Kind | Extensions | Time taken from |
| --- | --- | --- |
| JPEG | `.jpg` `.jpeg` `.jpe` | EXIF |
| HEIF | `.heic` `.heif` `.hif` | EXIF. Needs HEIF Image Extensions |
| RAW | `.dng` `.cr2` `.cr3` `.nef` `.arw` | EXIF. Needs Raw Image Extension |
| PNG, WebP | `.png` `.webp` | EXIF, when the file has it. WebP needs WebP Image Extensions |
| Video | `.mp4` `.mov` `.m4v` `.3gp` | The recording time stored in the file |
| AVI | `.avi` | The encoding date, when Windows can read it |

The extensions named above come from the Microsoft Store. PowerKit does not
install them.

## Good to know

- The name shows the local time at the place of capture. A phone photo taken
  abroad keeps the time you saw on the phone.
- Most videos store their time in UTC without a time zone. MediaRenamer
  converts it with the time zone of this computer, so a video recorded while
  travelling can be off by the difference between the two zones. Videos from
  an iPhone record their own time zone and are not affected.
- A video recorded on an Android phone is named by the time the recording
  started.
- PNG, WebP and AVI files often have no capture time and show "No date".
- Other files in a folder you add are skipped and counted, not renamed.
- HEIC, Live Photo and RAW files, missing extensions and online-only cloud
  files have not yet been checked with files from real devices. Look at the
  preview carefully before renaming such files.
