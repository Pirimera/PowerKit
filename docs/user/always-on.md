# AlwaysOn

[User guide](README.md) · [Türkçe](tr/always-on.md)

AlwaysOn keeps Windows from going to sleep while you need it awake: during a
long download, a presentation or a build. It can also keep the display on.

## Start and stop

Choose a mode on the Always On page or in the tray menu, or press
Win+Ctrl+Shift+A.

| Mode | Behavior |
| --- | --- |
| AlwaysOn off | Windows follows its own power settings |
| Indefinite | Stays awake until you turn it off |
| 30 minutes, 1 hour, 2 hours | Stays awake for that long, then turns off |
| Custom interval | Stays awake for the hours and minutes you set, up to 99 h 59 min. The default is 4 hours |
| Keep awake until | Stays awake until a date and time you choose. With a time but no date, a time that has already passed means tomorrow |
| Keep on while an app runs | Stays awake until an application exits |

The shortcut turns AlwaysOn off when it is on. When it is off, the shortcut
starts the mode you last chose since PowerKit started.

**Keep on while an app runs** follows the application whose window was active
last. Click that application's window first, then choose **Bind to last app**
on the page or the item in the tray menu.

The tray icon changes while AlwaysOn is on, and its tooltip shows the mode and
the time left.

## Options

| Option | What it does |
| --- | --- |
| Keep the display on too | Also stops the display from turning off. Without it only sleep is prevented, and the display may still turn off |
| Toggle shortcut | The shortcut that turns AlwaysOn on and off |
| Warn before ending | Shows a notification 5 minutes before a timed session ends. Clicking it adds 30 minutes. The tray menu has the same **Extend by 30 minutes** item |
| Only when plugged in | AlwaysOn does not start on battery, and stops when you unplug the computer |
| Stop when battery falls below | Stops AlwaysOn below the chosen level, in steps of 10% up to 90%. 0 turns the limit off. It is not applied while plugged in |

## From the command line

```powershell
powerkit --always-on on      # indefinitely
powerkit --always-on 90      # for 90 minutes
powerkit --always-on off
powerkit --always-on-pid 1234   # while process 1234 runs
```

The command controls the copy of PowerKit that is already running, or starts
PowerKit in the tray.

## Good to know

- Every start of PowerKit begins with AlwaysOn off. A session is never resumed
  after a restart; the options above are remembered.
- AlwaysOn prevents sleep caused by inactivity only. Putting the computer to
  sleep yourself, closing the lid and screen savers are not blocked.
- Time the computer spends asleep counts toward a timer. A session whose end
  passed during sleep is off when the computer wakes.
- To check that it works, run `powercfg /requests` in a terminal started as
  administrator. The request is listed as "PowerKit AlwaysOn".
- Updating PowerKit does not interrupt a running session.
