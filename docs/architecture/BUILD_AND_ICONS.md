# Build CI/CD i ikony aplikacji

## Pipeline budowania (GitHub Actions)

Workflow: `.github/workflows/build.yml`

### Platformy docelowe

| Platforma | Target | Format |
|-----------|--------|--------|
| Linux (Ubuntu 24.04) | x86_64-unknown-linux-gnu | deb, rpm, AppImage |
| Windows | x86_64-pc-windows-msvc | msi |
| macOS ARM | aarch64-apple-darwin | dmg |
| macOS Intel | x86_64-apple-darwin | dmg |

### Wyzwalacze

- **Push na `main`** - automatyczny build + release
- **Workflow dispatch** - ręczne uruchomienie z opcją `create_release`

### Concurrency

Ustawione `cancel-in-progress: true` - nowy push anuluje trwający build na tej samej gałęzi.

### Release

Każdy udany build tworzy release oznaczony jako **Latest** (nie draft, nie prerelease). Aplikacje 3rd-party mogą linkować do:

```
https://github.com/ReadyNextOs/ReadyNextOsDrive/releases/latest
```

## Ikony

### Pliki wymagane (`src-tauri/icons/`)

| Plik | Opis | Uwagi |
|------|------|-------|
| `app-icon.png` | Master 512x512 | Źródło do generowania pozostałych |
| `icon.ico` | Windows (multi-res) | Musi zawierać 16, 24, 32, 48, 64, 128, 256px |
| `icon.icns` | macOS (Apple Icon Image) | Musi mieć magic bytes `icns`, nie przemianowany PNG |
| `32x32.png` | PNG 32x32 | |
| `128x128.png` | PNG 128x128 | |
| `256x256.png` | PNG 256x256 | |
| `128x128@2x.png` | PNG 256x256 (Retina) | |
| `tray-icon.png` | Ikona tray domyślna | 32x32, używana też jako `NotConfigured` |
| `tray-idle.png` | Tray: zsynchronizowano | 32x32 |
| `tray-syncing.png` | Tray: synchronizacja | 32x32 |
| `tray-error.png` | Tray: błąd/konflikt | 32x32 |

### Generowanie ikon

Do regeneracji `icon.ico` i `icon.icns` z `app-icon.png` użyj Pythona z Pillow:

```bash
# icon.ico (multi-resolution)
python3 -c "
from PIL import Image
img = Image.open('src-tauri/icons/app-icon.png').convert('RGBA')
sizes = [16, 24, 32, 48, 64, 128, 256]
img.save('src-tauri/icons/icon.ico', format='ICO', sizes=[(s, s) for s in sizes])
"

# icon.icns (macOS)
python3 -c "
from PIL import Image
import struct, io
src = Image.open('src-tauri/icons/app-icon.png').convert('RGBA')
types = {16: b'icp4', 32: b'icp5', 64: b'icp6', 128: b'ic07', 256: b'ic08', 512: b'ic09'}
entries = b''
for size, ostype in sorted(types.items()):
    buf = io.BytesIO()
    src.resize((size, size), Image.LANCZOS).save(buf, format='PNG')
    png = buf.getvalue()
    entries += ostype + struct.pack('>I', 8 + len(png)) + png
data = b'icns' + struct.pack('>I', 8 + len(entries)) + entries
with open('src-tauri/icons/icon.icns', 'wb') as f: f.write(data)
"
```

### Walidacja w CI

CI nie generuje ikon — tylko sprawdza czy wszystkie wymagane pliki istnieją w repo. Jeśli brakuje jakiegokolwiek pliku, build kończy się błędem z komunikatem wskazującym brakujący plik.

### Ikony tray w kodzie Rust

Ikony tray są wbudowywane w binarke przez `include_bytes!` w `src-tauri/src/main.rs` (funkcja `update_tray_icon`). Zmiana ikon tray wymaga rekompilacji.

```rust
let icon_bytes: &[u8] = match status {
    SyncStatus::Idle => include_bytes!("../icons/tray-idle.png"),
    SyncStatus::Syncing => include_bytes!("../icons/tray-syncing.png"),
    SyncStatus::Error(_) => include_bytes!("../icons/tray-error.png"),
    SyncStatus::Conflict => include_bytes!("../icons/tray-error.png"),
    SyncStatus::NotConfigured => include_bytes!("../icons/tray-icon.png"),
};
```

### Znane problemy (historyczne)

1. **Niebieskie kwadraty na Windows** - CI bezwarunkowo nadpisywał ikony placeholderami `#1976d2`. Naprawiono w v0.3.1 przez usunięcie generatora placeholderów.
2. **icon.ico 16x16** - Oryginalny plik miał tylko 1 rozdzielczość (751B). Windows upscalował go z artefaktami. Naprawiono generując multi-res ICO (30KB, 7 rozmiarów).
3. **icon.icns jako PNG** - `shutil.copy('128x128.png', 'icon.icns')` tworzył fałszywy icns. Naprawiono generując prawdziwy Apple Icon Image format.

## Zależności Tauri 2.x

### Cargo.lock

`Cargo.lock` **musi** być commitowany do repo. Bez niego CI robi świeży resolve zależności i może pobrać niekompatybilne wersje pluginów.

### Feature flags

```toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
```

- `tray-icon` - obsługa ikony w zasobniku systemowym
- `image-png` - wymagane dla `Image::from_bytes()` z plikami PNG

### Wymagane importy Rust (Tauri 2.10+)

```rust
use tauri::Emitter;  // wymagane dla app.emit()
use tauri::Manager;  // wymagane dla app.get_webview_window(), app.state()
```

### Non-exhaustive enums

`CommandEvent` z `tauri-plugin-shell` jest `#[non_exhaustive]` — match musi zawierać `_ => {}`.
