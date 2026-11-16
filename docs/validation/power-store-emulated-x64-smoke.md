# PowerStore emulated x64 worker smoke

Date: 2026-11-16. Host: Windows 11 ARM64. Target:
`x86_64-pc-windows-msvc` release `powerkit.exe` from the local Cargo target
dir (emulation, not native x64). Winmd copied beside the executable.

Commands (child worker, no WinUI):

- `--power-store-worker installed` → exit 0, 155 nonempty lines ending in `OK`
- `--power-store-worker search-id Microsoft.WindowsTerminal` → exit 0, one
  `P\twinget\t…` row + `OK`
- `--power-store-worker search Microsoft.WindowsTerminal winget` → exit 0
- `--power-store-worker search terminal msstore` (and
  `Windows Terminal` / `WindowsTerminal`) → exit 0 with `OK` and no package
  rows (not a COM failure; no interactive `msstore` install attempted)

Also re-checked native ARM64 debug worker `search-id Microsoft.WindowsTerminal`
→ exit 0 with the same Terminal row shape.

Gaps unchanged: native x64, MSIX-packaged activation, UAC installers,
interactive `msstore` install, cancel during an external installer UI.
