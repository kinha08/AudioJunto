# AudioJunto

**One sound. Multiple devices.**

[Português brasileiro](README.pt-BR.md) · [Download](https://github.com/kinha08/AudioJunto/releases/latest) · [Report an issue](https://github.com/kinha08/AudioJunto/issues)

AudioJunto is a lightweight Rust application for **Windows 10 and 11 (64-bit)** that mirrors the sound playing on one output device to one or more additional devices. Listen together using two headphones, or share audio across headphones and speakers, with a simple interface and system tray controls.

## Features

- Play audio on two or more output devices, including the original output.
- Choose the source and any number of additional outputs detected by Windows.
- Enable or disable mirroring with one button or from the system tray.
- Keep the original output playing when mirroring is disabled.
- Close the window to keep the app running in the tray.
- Save device selections; mirroring always starts disabled.
- Keep a single instance per Windows session. Launching again restores the existing window.
- Use Windows audio APIs without installing a virtual audio driver or requiring administrator access.

The current interface and installer are in Brazilian Portuguese.

## Download and install

Open the [latest release](https://github.com/kinha08/AudioJunto/releases/latest) and choose:

| File | Purpose |
| --- | --- |
| `AudioJunto-Setup-0.1.3.exe` | Installer with a Start menu shortcut, optional desktop shortcut, and uninstaller. |
| `AudioJunto-0.1.3.exe` | Portable executable. Place it in a folder you can write to and run it. |
| `SHA256SUMS.txt` | Checksums for verifying the downloads. |

The installer installs for the current user. Settings are stored in `AudioJunto.ron` beside the application executable. To uninstall, use **Settings → Apps → Installed apps → AudioJunto**.

When upgrading from a version stuck in the tray, end its **AudioJunto** process in Task Manager before installing the update.

## Quick start

1. Connect both headphones or your other output devices.
2. In Windows sound settings, select the first device as your normal output and start playing audio.
3. Open AudioJunto and select that device under **Origem do áudio** (audio source).
4. Under **Tocar também em** (also play on), check the additional devices.
5. Click **Ativar** (enable). The original device keeps playing while AudioJunto sends copies to the selected outputs.
6. Click **Desativar** (disable) to stop the copies and keep only the original output.

Disable mirroring before changing devices. Use **Atualizar** (refresh) after connecting a new device. Closing the window hides it in the system tray; double-click the tray icon to reopen it. Right-click the icon to open the window, toggle mirroring, or choose **Sair** (quit).

AudioJunto does not change the Windows default output. Applications routed to a different device are not included in the mirrored sound.

## Latency and compatibility

Mirroring adds buffering and output-device latency. The original device plays directly, so the additional outputs can be slightly behind it. Bluetooth devices can introduce a larger delay, and different headphones may not be perfectly synchronized. This version has no manual delay alignment for the original output.

AudioJunto is intended for shared listening. It is less suitable for musical monitoring or other uses requiring tightly synchronized playback. Protected content, exclusive-mode applications, and incompatible drivers may prevent capture or playback. For Bluetooth headphones, prefer the stereo profile over the hands-free profile.

If an output fails, AudioJunto stops mirroring and reports the error. If the Windows default output changes, mirroring stops; check the source and enable it again.

## How it works

AudioJunto captures the source device's playback mix through **WASAPI loopback**, then opens additional outputs in shared mode. Audio uses 48 kHz stereo floating-point samples with format conversion handled by Windows. Each output has a bounded queue and adaptive interpolation to compensate for small hardware clock differences.

The app does not capture microphones, record audio to files, or transmit audio over the network.

## Build from source

Requirements:

- Windows 10/11 x64.
- Stable Rust with the `x86_64-pc-windows-msvc` toolchain.
- Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK.

```powershell
git clone https://github.com/kinha08/AudioJunto.git
cd AudioJunto
cargo test --locked
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked
.\target\release\audio-junto.exe
```

Static CRT linking avoids requiring a separate Visual C++ Redistributable installation for the distributed executable.

To build the installer, install [Inno Setup](https://jrsoftware.org/isinfo.php) with its Brazilian Portuguese translation, then run:

```powershell
Copy-Item target\release\audio-junto.exe installer\payload\AudioJunto.exe
New-Item -ItemType Directory -Force dist
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' "/O$PWD\dist" installer\AudioJunto.iss
```

Adjust the compiler path for your Inno Setup installation. The GitHub Actions release workflow tests the source, builds the application and installer, and publishes both executables and their checksums for a new version on `main`.

## Testing and contributing

Automated tests cover audio queue bounds, stereo samples, silence recovery, single-instance signaling, concurrent startup, and native notifications to a hidden window. They do not replace listening tests on real devices.

Before submitting a change, run `cargo fmt --check` and `cargo test --locked`. For audio or tray changes, also test playback, disabling mirroring, tray commands with the window hidden, device disconnection, and a Windows default-output change. Include your Windows version, device types, app version, and steps to reproduce when [reporting an issue](https://github.com/kinha08/AudioJunto/issues).

## License

[MIT](LICENSE) © 2026 Lucas Nobre.
