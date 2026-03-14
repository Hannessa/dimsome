# Dimsome

Dimsome is a Windows tray app for software-based screen dimming. It applies dimming overlays, allowing you to get a darker screen than your monitor normally allows, and lets you control them through a settings window, scheduled transitions, and global hotkeys.

## What it does

- Runs from the Windows system tray
- Applies software dimming overlays across displays
- Supports scheduled dim levels with smooth fade durations
- Lets you temporarily override the schedule with manual dimming
- Registers global hotkeys for dim more and dim less actions
- Can launch automatically at Windows sign-in

## Tech stack

- Vue 3 + TypeScript frontend in `src/`
- Tauri 2 desktop shell and IPC bridge
- Rust backend in `src-tauri/`
- Windows-specific integration through the `windows` crate

## Project structure

- `src/`: Vue UI for the settings window
- `src-tauri/src/`: native app runtime, tray behavior, settings, schedule logic, hotkeys, and startup integration
- `src-tauri/icons/`: application icons used for bundling
- `src-tauri/gen/`: generated Tauri schema files

## Development

Install dependencies:

```powershell
npm install
```

Run the frontend by itself:

```powershell
npm run dev
```

Run the full desktop app in development mode:

```powershell
npm run tauri:dev
```

Build the frontend bundle:

```powershell
npm run build
```

Create a production desktop build:

```powershell
npm run tauri:build
```

## Configuration and persistence

Dimsome stores app settings at:

```text
%LOCALAPPDATA%\Dimsome\settings.json
```

## Notes

- This project is currently focused on Windows behavior.
- Tauri bundles are configured through [`src-tauri/tauri.conf.json`](./src-tauri/tauri.conf.json).
- Rust dependencies are locked in [`src-tauri/Cargo.lock`](./src-tauri/Cargo.lock), and Node dependencies are locked in [`package-lock.json`](./package-lock.json).


## License

Dimsome is open source under the [MIT License](./LICENSE).

