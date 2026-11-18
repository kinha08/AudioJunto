# Changelog

## 0.1.3

- Use the AudioJunto icon in the native title bar.
- Replace the unsupported header glyph with the same icon rendered as an image.
- Share the icon pixels between the title bar, header, and system tray.

## 0.1.2

- Wake the Windows event loop directly when a tray command arrives, including while the main window is hidden.
- Restore minimized windows when opening from the tray.
- Refresh tray menu text only when the audio state changes.
- Preserve single-instance behavior and add a hidden-window notification regression test.

## 0.1.1

- Allow one running instance per Windows session.
- Signal the running instance when launched again and restore its window.
- Test duplicate startup, concurrent launches, and relaunch after shutdown.

## 0.1.0

- Initial Windows audio mirroring application with source selection, multiple additional outputs, a dark interface, tray controls, and a per-user installer.
