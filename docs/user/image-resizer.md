# ImageResizer

[User guide](README.md) · [Türkçe](tr/image-resizer.md)

ImageResizer resizes one image or a whole selection in one step. It always
shows what it will write before it writes anything.

## Resize images

1. In File Explorer, select one or more images, right-click and choose
   **Resize with Image Resizer**. You can also choose **Select images here…**
   on the Image Resizer page, or **Resize images** in the tray menu.
2. In the window that opens, choose a size.
3. Choose **Preview**. The list shows the new name and the new dimensions of
   every file. Nothing is written yet.
4. Choose **Resize**.

Changing a size or an option after the preview makes the preview out of date;
preview again before resizing. The work can be cancelled while it runs; the
files that were not reached are left untouched.

## Sizes

| Size | Fits within |
| --- | --- |
| Small | 854 × 480 pixels |
| Medium | 1366 × 768 pixels |
| Large | 1920 × 1080 pixels |
| Phone | 320 × 568 pixels |
| Custom | The width and height you enter, in pixels, percent, inches or centimetres |

A custom size also has a fit:

| Fit | Result |
| --- | --- |
| Fit | The image is scaled to fit inside the size and keeps its proportions |
| Fill | The image fills the size and is cropped where it does not fit |
| Stretch | The image takes exactly the size, without keeping its proportions |

## Options

The window opens with the options from the Image Resizer page, and changes you
make in the window are saved there too.

| Option | What it does |
| --- | --- |
| Shrink only, do not enlarge | Leaves images that are already smaller than the size as they are |
| Overwrite original images | Writes the result over the original; see below |
| Ignore orientation | Treats width and height as interchangeable, so a portrait image is fitted to the portrait form of the size. On by default |
| Remove data that does not affect the image | Removes metadata such as camera details from the result |
| Keep modified date | The result keeps the original's modified date |
| File name | The name of the copy. Default `%1 (%2)` |
| JPEG quality | 1 to 100. Default 90 |
| PNG interlacing, TIFF compression | Encoding details for those formats |
| Fallback encoder | The format used when the original's format cannot be written. Default JPEG |

File name parts:

| Part | Stands for |
| --- | --- |
| `%1` | The original file name |
| `%2` | The name of the size |
| `%3`, `%4` | The width and height you selected |
| `%5`, `%6` | The width and height of the result |

## What happens to the original

- By default ImageResizer writes a copy next to the original, for example
  `Holiday (Small).jpg`. A copy never overwrites an existing file; a number is
  added to the name when it is taken.
- With **Overwrite original images**, the original is first kept as a `.bak`
  file. After the new image is written, ImageResizer tries to move that backup
  to the Recycle Bin.

## Good to know

- Supported files: BMP, DIB, GIF, HEIC, JPEG, PNG, TIFF, WDP and WebP. HEIC
  and WebP are read through Windows codecs. If such a file cannot be read,
  install the HEIF Image Extensions or WebP Image Extensions from the
  Microsoft Store.
- One run takes at most 1000 files.
- Only the first frame of a file with several frames, such as an animated GIF,
  is resized.
- ImageResizer does not enlarge images with AI.
