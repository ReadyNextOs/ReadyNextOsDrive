# Changelog

Wszystkie istotne zmiany w projekcie Veloryn CloudFile.

## [0.3.1] - 2026-04-08

### Naprawione
- Ikony Windows: wygenerowano `icon.ico` z 7 rozdzielczościami (16-256px) zamiast pojedynczego 16x16 niebieskiego kwadratu
- Ikona macOS: wygenerowano prawdziwy `icon.icns` (format Apple Icon Image) zamiast przemianowanego PNG
- CI: usunięto krok bezwarunkowego nadpisywania ikon placeholderami, zastąpiono walidacją obecności plików
- Kompatybilność z Tauri 2.10: dodano feature `image-png`, import `tauri::Emitter`, poprawiono `FilePath::to_string()`
- Kompilacja: dodano wildcard arm dla `#[non_exhaustive]` enum `CommandEvent`

## [0.3.0] - 2026-04-08

### Dodane
- Pinowanie zależności Rust przez commitowanie `Cargo.lock`
- Release automatycznie oznaczany jako Latest (nie draft/prerelease)

### Zmienione
- Rebrand: ReadyNextOs Drive -> Veloryn CloudFile

## [0.2.0] - 2026-02-09

### Dodane
- Hardened sync flow z background schedulerem
- Dokumentacja architektury
- Autostart, notyfikacje systemowe, tray icon

## [0.1.0] - 2026-02-09

### Dodane
- Pierwsza wersja aplikacji
- Synchronizacja plików przez rclone (sidecar)
- Logowanie przez token desktopowy
- Interfejs React z konfiguracją folderów
