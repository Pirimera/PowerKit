# Package manager icons

Icons shown for PowerStore's package managers in the settings drawers and the
Source column of the package list.

## Source

Copied unmodified from UniGetUI, <https://github.com/Devolutions/UniGetUI>, commit
`6b766e194a1197d4a3db2649c2c7b73d5b57aa50`, directory
`src/SharedAssets/Assets/Images/`, except where noted:

| File here | Upstream file | Used for |
| --- | --- | --- |
| `winget.png` | `winget_color.png` | WinGet |
| `scoop.png` | `scoop_color.png` | Scoop |
| `chocolatey.png` | `choco_color.png` | Chocolatey |
| `npm.png` | not in UniGetUI; the npm organization avatar, <https://github.com/npm> (`https://avatars.githubusercontent.com/u/6078720`) | npm |
| `pip.png` | `pip_color.png` | pip |
| `cargo.png` | `cargo_color.png` | Cargo |
| `dotnet.png` | `dotnet_color.png` | .NET Tool |
| `powershell.png` | `powershell_color.png` | Windows PowerShell and PowerShell 7 |
| `vcpkg.png` | `vcpkg_color.png` | vcpkg |

UniGetUI maps npm to its Node.js logo (`node_color.png`). PowerKit uses the npm
logo instead, because the Node.js logo does not identify the npm manager.

## License

UniGetUI is published under the MIT License, Copyright (c) 2025 Martí Climen. The
permission notice below applies to these copies.

```text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The MIT License covers UniGetUI's copies only. The logos themselves remain marks of
their projects (Microsoft, Scoop, Chocolatey Software, npm, the Python Software
Foundation, the Rust Foundation, the .NET Foundation and Microsoft vcpkg). PowerKit
uses them only to identify the matching package manager.
