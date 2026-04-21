# QsoRipper Keyboard Shortcuts Reference

All three QsoRipper surfaces — GUI (Avalonia), TUI (ratatui), and Win32 — share a
keyboard-first design philosophy. This document is the authoritative reference for
shortcuts across surfaces.

## Navigation

| Action | GUI | TUI | Win32 | Notes |
|---|---|---|---|---|
| Help / shortcuts | F1 | F1 | F1 | All surfaces: overlay with full shortcut list |
| Focus QSO grid/list | F3 | F3 | F3 | Consistent across all surfaces |
| Focus search | F4 or Ctrl+F | F4 | F4 | GUI adds Ctrl+F alias |
| Focus QSO logger | Ctrl+N | Tab (cycles) | Tab (cycles) | GUI has explicit focus command |
| Quit | Ctrl+Q or Alt+X | Ctrl+Q | Ctrl+Q | GUI adds Alt+X (Windows convention) |
| Close overlay / cancel | Esc | Esc | Esc | Consistent: closes topmost overlay, then clears logger |

## QSO Logging

| Action | GUI | TUI | Win32 | Notes |
|---|---|---|---|---|
| Log QSO | Ctrl+Enter or F10 | F10 or Alt+Enter | F10 or Shift+Enter or Alt+Enter | F10 is shared across all; GUI adds Ctrl+Enter |
| Reset QSO timer | F7 | F7 | F7 | Consistent across all surfaces |
| Cycle band | ←/→ on band button | ←/→ in band field | ←/→ in band field | Same gesture, different control types |
| Cycle mode | ←/→ on mode button | ←/→ in mode field | ←/→ in mode field | Same gesture, different control types |
| Jump to callsign | Alt+C | Alt+C | Alt+C | Consistent across all surfaces |
| Jump to band | Alt+B | Alt+B | Alt+B | Consistent across all surfaces |
| Jump to mode | Alt+M | Alt+M | Alt+M | Consistent across all surfaces |
| Open QSO Card | Alt+A or Ctrl+L | — | — | GUI only (overlay form); TUI/Win32 show fields inline |

## Grid / QSO List

| Action | GUI | TUI | Win32 | Notes |
|---|---|---|---|---|
| Edit selected cell | F2 | — | — | GUI only (DataGrid inline edit) |
| Save pending edits | Ctrl+S | — | — | GUI only (commit grid edits) |
| Delete selected QSO | Ctrl+D or Delete | D or Delete | D or Delete | GUI requires Ctrl+D or Delete key |
| Refresh | F5 | — | — | GUI only |
| Callsign lookup card | F8 | — | — | GUI only (side panel with QRZ data) |
| Toggle inspector | Alt+Enter | — | — | GUI only (detail panel) |
| Zoom in | Ctrl++ | — | — | GUI only |
| Zoom out | Ctrl+- | — | — | GUI only |
| Reset zoom | Ctrl+0 | — | — | GUI only |

## System

| Action | GUI | TUI | Win32 | Notes |
|---|---|---|---|---|
| Sync with QRZ | F6 | — | — | GUI only |
| Toggle rig control | Ctrl+R | F8 | F8 | Different key: GUI uses Ctrl+R, TUI/Win32 use F8 |
| Toggle space weather | Ctrl+W | — | — | GUI only |
| Settings | Ctrl+, | — | — | GUI only (dialog) |
| Sort chooser | Ctrl+Shift+S | — | — | GUI only |
| Column chooser | Ctrl+H | — | — | GUI only |

## QSO Card (GUI only)

| Action | Shortcut | Notes |
|---|---|---|
| Switch to tab N | Alt+1..6 | 1=Core, 2=Lookup, 3=QSL, 4=Contest, 5=Station, 6=Metadata |
| Next tab | Ctrl+Tab | Wraps around |
| Previous tab | Ctrl+Shift+Tab | Wraps around |
| Save QSO | Ctrl+Enter, Ctrl+S | Saves and closes card |
| Close card | Esc | Discards changes |

## Settings (GUI only)

| Action | Shortcut | Notes |
|---|---|---|
| Switch to section N | Ctrl+1..5 | Numbered sections |
| Next section | Ctrl+Tab | Wraps around |
| Previous section | Ctrl+Shift+Tab | Wraps around |
| Save | Enter (on Save button) | |
| Cancel / close | Esc | |

## Surface-Specific Differences

### F2 — Edit Cell (GUI) vs Toggle Advanced (TUI/Win32)
In TUI and Win32, F2 toggles between the basic and advanced log form views.
The GUI uses F2 for DataGrid cell editing because the advanced form is a
separate overlay (Alt+A). This is an intentional divergence.

### F5/F6 — Refresh/Sync (GUI) vs Advanced Tabs (TUI/Win32)
In TUI and Win32, F5/F6 cycle between advanced form tabs. The GUI uses F5
for grid refresh and F6 for QRZ sync. The QSO card uses Ctrl+Tab and Alt+1..6
for tab switching instead.

### F8 — Callsign Card (GUI) vs Rig Toggle (TUI/Win32)
In TUI and Win32, F8 toggles rig control. The GUI uses F8 for the callsign
lookup card and Ctrl+R for rig control. This gives the GUI a dedicated key
for the frequently-used callsign lookup without losing rig control access.

### Alt+Enter — Inspector (GUI) vs Log QSO (TUI/Win32)
In TUI and Win32, Alt+Enter logs or updates a QSO. The GUI uses Alt+Enter
for the QSO inspector panel and Ctrl+Enter/F10 for logging. This avoids
conflict with the menu access key system where Alt activates the menu bar.

## Discoverability Features

### Contextual Hint Strip (GUI)
The GUI status bar includes a right-aligned contextual hint strip that changes
based on the current focus area:

| Focus Area | Hints Shown |
|---|---|
| QSO Grid | F2 edit · Ctrl+D delete · F8 lookup · Alt+Enter inspect · F5 refresh |
| Logger | Alt+C call · Alt+B band · Alt+M mode · F10 log · Esc clear · Alt+A card |
| Search | Esc clear · Enter search · F3 grid · Ctrl+N logger |

### Inline Shortcut Hints (GUI)
Toolbar buttons show their keyboard shortcuts directly on the button face
(e.g. "Sort Ctrl+⇧+S", "Cols Ctrl+H", "⟳ Sync F6") so they are discoverable
without hovering for tooltips.

### Post-Log Focus Return (GUI)
After logging a QSO with F10 or Ctrl+Enter, the GUI automatically clears the
form and returns focus to the callsign field for immediate next-contact entry.
This matches the TUI behavior for rapid contest logging.
