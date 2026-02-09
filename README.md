# ReadyNextOs Drive

Desktop file synchronization client for ReadyNextOs. Built with [Tauri v2](https://v2.tauri.app/) + React + TypeScript.

Uses [rclone](https://rclone.org/) as a sidecar for WebDAV-based file sync with the ReadyNextOs backend.

## Development

### Prerequisites

- [Node.js 22+](https://nodejs.org/)
- [Rust](https://rustup.rs/)
- System dependencies (Linux): `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`

### Setup

```bash
npm install
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

## Downloads

Pre-built installers are available from [GitHub Releases](../../releases):

| Platform | Format |
|----------|--------|
| Windows | `.msi` |
| macOS (ARM) | `.dmg` |
| macOS (Intel) | `.dmg` |
| Linux (Debian/Ubuntu) | `.deb` |
| Linux (universal) | `.AppImage` |

## License

Proprietary - ReadyNextOs
