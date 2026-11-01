# PowerKit

Native Windows 11 utilities. AlwaysOn and AlwaysTop are currently available;
the interface is Turkish.

Choose indefinite mode or a 30/60/120-minute timer. Enable the display option to keep
the screen awake too. The X button hides the window when a tray icon is available;
use the explicit exit button to stop the app. Manual sleep and closing the lid are
not blocked. AlwaysOn starts off after every launch. AlwaysTop pins the last
focused window from the tray or Win+Ctrl+T and also starts off after every launch.

Display sleep and AlwaysTop sound are stored in %LOCALAPPDATA%\\PowerKit\\settings.txt.
Updates are downloaded in the background and applied on normal exit. To restart
for an update immediately, stop AlwaysOn first.

Private repository releases cannot be downloaded anonymously by the app.
Use your authorized GitHub access to install updates until the repository is public.
Initial installers are unsigned.

Test-AlwaysOn.ps1 runs an isolated smoke check; it does not verify actual sleep,
installation, uninstallation or installed-version upgrades.

See [the project](https://github.com/Pirimera/PowerKit) for source, contribution
guidelines and the physical acceptance checklist. MIT terms are in LICENSE.
Third-party attribution is in THIRD-PARTY-NOTICES.txt.
