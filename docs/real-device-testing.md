# Real-device test checklist

For v1.0 sign-off. Run it once on **Windows 11** and once on **Ubuntu LTS** (22.04 or 24.04), using the latest release from the [Releases page](https://github.com/AffanTariq77/repakhub/releases). Automated tests already cover parsing, versions, signed updates, downloads and backups against fixtures. What only a real machine can show is whether RepakHub finds real launchers' games correctly.

How to record results: tick each box that passes. For anything that fails, write in **Notes** what you did, what you expected and what happened, and take a screenshot. Note the launcher's version too, because launcher file formats change. Never paste a Steam Web API key, account names or file contents from your saves into a report.

## Before you start

Set up as much of this as you can. Skip a row's checks if you cannot set it up, and mark them "not tested".

| | Windows 11 | Ubuntu |
| --- | --- | --- |
| Steam with games in **two library folders on different drives**, at least one game with an update pending (pause auto-updates for it) | ☐ | ☐ |
| Steam installed somewhere other than the default folder (tests the registry lookup) | ☐ | n/a |
| Steam installed both natively and as a Flatpak | n/a | ☐ (optional) |
| A Proton game (Steam Linux Runtime and Proton should not be listed as games) | n/a | ☐ |
| GOG Galaxy with an installed game, ideally one with DLC | ☐ | n/a |
| A GOG game installed from its offline installer, without Galaxy | ☐ | ☐ |
| Epic Games Launcher with an installed game | ☐ | n/a |
| Heroic with a GOG game and an Epic game installed | ☐ (optional) | ☐ |
| A folder of DRM-free games (for example `D:\DRM-free\<game>`), each in its own subfolder | ☐ | ☐ |
| A USB drive holding a Steam library (for the offline-drive test) | ☐ | ☐ |
| A Steam Web API key from steamcommunity.com/dev/apikey (for owned games) | ☐ | ☐ |

Machine details, for the report:

| | Windows 11 | Ubuntu |
| --- | --- | --- |
| OS version | | |
| RepakHub version | | |
| Steam, Galaxy, Epic, Heroic versions | | |

## 1. Install and first run

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 1.1 | Windows: run the `-setup.exe`. If SmartScreen warns, choose "More info", then "Run anyway". Ubuntu: install the `.deb` with `sudo apt install ./RepakHub_*.deb`, or make the `.AppImage` executable and run it | RepakHub opens on "Where are your games?" | ☐ | ☐ |
| 1.2 | Look at the drive list | Every local drive is listed (Windows: `C:\`, `D:\`, ...; Ubuntu: home and mounted drives). Mapped network drives are not | ☐ | ☐ |
| 1.3 | Select the drives that hold games, leave "Show cover art" on, continue | The library opens and a scan starts | ☐ | ☐ |
| 1.4 | Ubuntu only: run `ls -ld ~/.local/share/co.repakhub.desktop` | `drwx------` (only you can read RepakHub's data) | n/a | ☐ |
| 1.5 | Start RepakHub a second time while it is open | The open window comes to the front; no second window | ☐ | ☐ |

## 2. Finding installed games (acceptance: "Scan selected drives")

For each launcher, compare RepakHub's library with the launcher's own list of installed games.

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 2.1 | Steam games, in both library folders | Every installed Steam game is listed once. Nothing missing, nothing extra | ☐ | ☐ |
| 2.2 | Steam outside the default folder | Its games are found without adding that folder as a scan location | ☐ | n/a |
| 2.3 | Steam tools on Ubuntu | Proton, Steam Linux Runtime and Steamworks Common Redistributables are not listed | n/a | ☐ |
| 2.4 | Steam native and Flatpak with the same game in both | Known limitation: listed once. Note which copy appears | n/a | ☐ |
| 2.5 | GOG Galaxy games | Listed, with the version Galaxy shows | ☐ | n/a |
| 2.6 | GOG offline-installer game | Listed as GOG, with a version or "Unknown" | ☐ | ☐ |
| 2.7 | Epic Games Launcher games | Listed, with a version | ☐ | n/a |
| 2.8 | Heroic GOG and Epic games | Listed as GOG and Epic | ☐ | ☐ |
| 2.9 | DLC | DLC is not listed as a separate game | ☐ | ☐ |
| 2.10 | Settings → Game folders → "Add game folder…", choose the DRM-free folder, Save, then "Scan for games" | Each subfolder appears as one game named after the folder, source "Game folder". A GOG game inside that folder is listed as GOG, not twice | ☐ | ☐ |
| 2.11 | Open Game Details for a few games and press "Show folder" | The right folder opens | ☐ | ☐ |
| 2.12 | Time the first scan | Finishes in under a minute (note the time and game count) | ☐ | ☐ |

## 3. Rescans and changes (acceptance: "Rescan")

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 3.1 | Press "Scan for games" three times | Same games, no duplicates | ☐ | ☐ |
| 3.2 | Uninstall a game in its launcher, then rescan | It disappears | ☐ | ☐ |
| 3.3 | Install a new game in a launcher, then rescan | It appears | ☐ | ☐ |
| 3.4 | Unplug the USB drive with a Steam library, rescan | Its games stay listed (an unplugged drive is not an uninstall) | ☐ | ☐ |
| 3.5 | Plug it back in, rescan | Still listed once | ☐ | ☐ |
| 3.6 | Settings → untick a scan location that holds games, Save, rescan | Games found only through that location disappear | ☐ | ☐ |

## 4. Restart (acceptance: "Restart")

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 4.1 | Close RepakHub and open it again | Library, scan locations, game folders and settings are all as you left them, before any scan | ☐ | ☐ |
| 4.2 | Restart the computer and open RepakHub | Same | ☐ | ☐ |

## 5. Versions (acceptance: "Version detection", "Unknown version")

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 5.1 | For five games, compare the version in Game Details with the launcher | They match, and the version's source (hover, or the text under it) says where it came from | ☐ | ☐ |
| 5.2 | Find a game whose version cannot be known | Shows "Unknown", never a guess | ☐ | ☐ |
| 5.3 | In Game Details, enter a version by hand and Save. Rescan. Restart | Your version is shown and survives both. "Clear" removes it | ☐ | ☐ |
| 5.4 | If any game says "Sources disagree" | Note the game and the values shown | ☐ | ☐ |

## 6. Updates (acceptance: "Update available")

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 6.1 | The Steam game with a pending update | Library card says "Update pending"; the Updates item in the sidebar shows a count | ☐ | ☐ |
| 6.2 | On first finding it | One system notification names the game (allow notifications if asked) | ☐ | ☐ |
| 6.3 | Restart RepakHub | No second notification for the same update | ☐ | ☐ |
| 6.4 | Updates page → "Open in Steam" | Steam opens on that game | ☐ | ☐ |
| 6.5 | Let Steam install the update, rescan | "Update pending" goes away | ☐ | ☐ |
| 6.6 | GOG and Epic games on the Updates page | Counted as "does not report updates", not shown as up to date | ☐ | ☐ |

## 7. Owned games and the store

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 7.1 | Settings → add your Steam Web API key, Save, open My Library | "Owned, not installed" lists Steam games you own but have not installed | ☐ | ☐ |
| 7.2 | With GOG Galaxy signed in (Windows) or Heroic signed in | Owned GOG (and, with Heroic, Epic) games are listed too | ☐ | ☐ |
| 7.3 | Press a card's launcher button ("Open in Steam", "Open in GOG Galaxy", ...) | The launcher opens on that game | ☐ | ☐ |
| 7.4 | A game you own on two platforms | One card with a button for each platform | ☐ | ☐ |
| 7.5 | A game installed from two sources | One library card listing both sources; Game Details shows "Also installed" and switches between them | ☐ | ☐ |
| 7.6 | Store page → search "witcher" | GOG results with covers and prices; choosing one opens gog.com in your browser | ☐ | ☐ |
| 7.7 | Settings → untick "Show cover art", Save | Name tiles instead of images everywhere. Tick it again: Steam and GOG covers return | ☐ | ☐ |
| 7.8 | Remove the Steam key, Save | Owned Steam games disappear from the library | ☐ | ☐ |

## 8. DLC and backups (acceptance: "Restore backup")

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 8.1 | DLC & Add-ons page | Installed DLC listed under its game (Steam DLC by id, GOG and Epic by name) | ☐ | ☐ |
| 8.2 | Game Details → "Saves & config" → "Add folder…" → the game's save folder → back up | A backup appears with its date | ☐ | ☐ |
| 8.3 | Change or delete a save file, then "Restore" that backup | The save is back as it was. An automatic backup of the state before restoring is also listed | ☐ | ☐ |
| 8.4 | Launch the game | It loads the restored save | ☐ | ☐ |

## 9. Installing and updating from an authorised source (acceptance: "Interrupted download", "Invalid manifest", "Corrupted download")

The demo source is a small pack signed by RepakHub. Invalid manifests and corrupted downloads are covered by automated tests; here we check the real flow.

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 9.1 | Integrations → Authorised sources → the demo pack → "Install to…" → choose a folder | Downloads page shows progress, then it appears in My Library as a RepakHub install | ☐ | ☐ |
| 9.2 | Start the install again into another folder, press "Pause" during the download, wait, then "Resume" | It finishes and the files are correct (the pack is small, so this may finish before you can pause; note if so) | ☐ | ☐ |
| 9.3 | During a download, switch off the network for 30 seconds, then switch it back on | The download continues, or fails with "Retry", which then completes | ☐ | ☐ |

## 10. Integrations

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 10.1 | Integrations page | Every adapter says what it reads; launcher adapters say "Never uses the network". Online features list their hosts | ☐ | ☐ |
| 10.2 | Switch off GOG | Its games leave the library | ☐ | ☐ |
| 10.3 | Switch it back on | They return | ☐ | ☐ |

## 11. RepakHub's own updates

Needs an older release installed first.

| # | Do this | Expect | Win | Ubuntu (AppImage) |
| --- | --- | --- | --- | --- |
| 11.1 | Install the previous release, open it | A banner offers the latest version | ☐ | ☐ |
| 11.2 | "Install and restart" | Progress, then RepakHub restarts on the new version (check the version in Windows Settings → Apps; on Ubuntu, the banner no longer appears on the next start) | ☐ | ☐ |
| 11.3 | Settings → untick "Check for RepakHub updates on startup", restart | No banner | ☐ | ☐ |
| 11.4 | Ubuntu `.deb` install | No banner offers to replace it; it updates through the package instead | n/a | ☐ |

## 12. Uninstall

| # | Do this | Expect | Win | Ubuntu |
| --- | --- | --- | --- | --- |
| 12.1 | Uninstall RepakHub (Windows: Settings → Apps; Ubuntu: `sudo apt remove repakhub`) | Removed. Your games, launchers and saves are untouched | ☐ | ☐ |

## Sign-off

| Acceptance criterion (docs/PLAN.md) | Sections | Windows 11 | Ubuntu |
| --- | --- | --- | --- |
| Scan selected drives | 2 | ☐ | ☐ |
| Rescan | 3 | ☐ | ☐ |
| Restart | 4 | ☐ | ☐ |
| Version detection | 5.1 | ☐ | ☐ |
| Unknown version | 5.2 | ☐ | ☐ |
| Update available | 6 | ☐ | ☐ |
| Interrupted download | 9.2, 9.3 | ☐ | ☐ |
| Invalid manifest, corrupted download | automated tests | ☐ | ☐ |
| Restore backup | 8 | ☐ | ☐ |
| Windows and Linux | all | ☐ | ☐ |

Tested by: ______________ Date: ______________
