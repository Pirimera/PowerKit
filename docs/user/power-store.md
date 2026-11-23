# PowerStore

[User guide](README.md) · [Türkçe](tr/power-store.md)

PowerStore shows the software installed on your computer in one table and
installs, updates and removes it through the package managers you already
have. It does not replace those managers; it runs them for you.

## Package managers

| Manager | Notes |
| --- | --- |
| WinGet | Part of Windows as App Installer. Covers the `winget` and `msstore` sources |
| Scoop | |
| Chocolatey | Version 2.0 or later |
| npm, pip, Cargo, .NET Tool | Globally installed tools only, not the packages of a project |
| Windows PowerShell, PowerShell 7 | Installed modules |
| vcpkg | |

A manager that is not installed is left out of the table. PowerStore does not
install managers; its settings show the command or link to install each one.

## The package table

The Power Store page lists installed packages. Packages with an update are
grouped at the top under **Updates available**, followed by **Installed
applications**. The last list is shown at once when you open the page, and the
managers are asked again in the background.

| Control | What it does |
| --- | --- |
| Filter box | Narrows the list by name or package ID as you type. It searches the list, not the online catalogs |
| Sources | Shows only the chosen sources |
| All statuses, Updates available, Up to date, Ignored / Skipped | Filters by update status |
| All scopes | Filters by packages installed for you or for the whole computer |
| Sort | By name, package ID, install date or last update |
| Refresh | Asks the managers again |

Each row has one main button (**Update**, **Uninstall**, **Install** or
**Info**) and a **⋯** menu with the other commands. Tick rows to act on
several packages with **Update selected**, **Uninstall** or **Ignore
updates**. **Update all** updates the packages that match the current
filters.

## Find and install new software

Choose **Find Packages**, type a name and search. The results appear in the
same table; **Install** adds one, or tick several and choose **Install
selected**. **Back to installed** returns to your list.

## Updates

PowerStore checks for updates in the background, first one minute after
PowerKit starts and then at the interval you set. It shows the count in the
tray menu, for example "Power Store (3)", and a notification when new updates
appear. **It never installs an update by itself.**

For a package you do not want to update:

| Command | Effect |
| --- | --- |
| Skip this version | Hides the offered version. A later version is offered again |
| Ignore updates | Hides all updates for the package until you choose **Stop ignoring** |

## Operations

Installs, updates and removals run one at a time, in the order you started
them. The row shows the progress, or **Waiting** for a queued package.

- The **⋯** button of a running package cancels it, and that of a waiting
  package removes it from the queue. **Cancel pending** clears the queue.
- An installer may ask for administrator permission in its own window.
  PowerStore itself never runs as administrator.
- Once an installer has opened its own window, cancelling may only take effect
  when that installer finishes its current step.
- **History** lists finished operations with their output, which helps when an
  installation fails. It keeps the 200 most recent operations.

## Package details and install options

Click a package's name to open its details, such as publisher, license and
homepage. For managers that support them, the same panel has install
options, which PowerStore uses for the installation and for later updates of
that package:

| Option | Available for |
| --- | --- |
| Scope (User or Machine), Architecture | WinGet |
| Version | WinGet, Scoop, Chocolatey, pip |
| Extra installer arguments | WinGet, Chocolatey |

## Settings

The gear button opens the settings.

| Setting | What it does |
| --- | --- |
| Package managers | A switch for each manager. A manager that is off is left out of the list, search and update checks. The arrow beside it opens that manager's own settings: the program to use, and for WinGet a default scope and architecture, for vcpkg a default triplet |
| Update check interval | From 15 minutes to 7 days |
| Notify about updates | Shows the notification described above |
| Remote package logos | Downloads missing logos from the UniGetUI icon database. On by default; needs the network |
| Ignored packages, Skipped versions | The lists you built with the commands above, with a button to remove an entry |
| Export, Import | Saves the installed packages to a file, or installs packages from such a file |

**Export and import:** the file has the format of `winget export`, so
`winget import` can read its WinGet part. Packages of other managers and your
install options are stored in a separate section. Import shows every package
with a check box and installs only the ones you leave ticked. Only packages a
manager can install again are exported.

## Good to know

- Software that no package manager knows appears with the source `local`. It
  can be listed but not updated here.
- Apps delivered by the Microsoft Store show **Update in Store**, which opens
  the Store's own updates page. They cannot be uninstalled from PowerStore.
- PowerKit itself is not updated or removed by PowerStore; the Store does
  that.
- If WinGet is reported as not installed or too old, install or update **App
  Installer** from the Microsoft Store.
- PowerStore does not detect updates for Cargo, .NET tool, vcpkg and
  PowerShell packages. **Update** on such a package asks the manager for the
  newest version.
- Not yet checked by hand: installing from the `msstore` source, installers
  that ask for administrator permission, and the Chocolatey and vcpkg
  managers on a computer that has them.
