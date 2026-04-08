# Changelog

Wszystkie istotne zmiany w projekcie Veloryn CloudFile.

## [0.5.0] - 2026-04-08

### Naprawione (Code Review)
- **CRITICAL**: Race condition w synchronizacji — atomowe sprawdzenie i ustawienie statusu pod jednym lockiem
- **CRITICAL**: Keychain Windows — login failuje jeśli token nie przeszedł weryfikacji; wymuszony natywny backend (windows-native)
- Przeniesienie env::remove_var przed start async runtime (bezpieczeństwo wątkowe)
- Blocking I/O na async runtime → tokio::fs + spawn_blocking
- Walidacja ścieżek Windows — blokowanie rootów dysków (C:\, D:\)
- Dodano `shell:default` do capabilities
- CSP `connect-src` zawężone z `https:` do `self`
- Odczyt logów: tail pliku (max 256KB) zamiast wczytywania całości do pamięci

### Dodane
- Szczegółowe logowanie operacji keychain (store/get/verify) do diagnostyki
- Przycisk "Otwórz plik logu" w sekcji Diagnostyka

### Usunięte
- Nieużywana zależność `env_logger`

## [0.4.0] - 2026-04-08

### Dodane
- Tryb debug z logowaniem do pliku (Ustawienia > Diagnostyka)
- Podgląd logów bezpośrednio w aplikacji
- Wyświetlanie błędów synchronizacji w UI (zamiast połykania w console.error)

### Naprawione
- Status "Nie skonfigurowano" po zalogowaniu — teraz poprawnie ustawia się na "Zsynchronizowane"
- Status przy starcie aplikacji z zapisaną konfiguracją

### Zmienione
- Kompaktowy layout strony Ustawienia — mieści się na ekranie bez przewijania
- Przyciski "Zapisz" i "Wyloguj" w jednym rzędzie

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
