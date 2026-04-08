# ReadyNextOs Drive — Przegląd UX/UI

**Data:** 2026-04-08
**Wersja:** 0.2.0
**Referencja:** Synology Drive Client 3.x
**Cel:** Doprowadzić aplikację do poziomu profesjonalnego klienta synchronizacji plików działającego na Windows, macOS i Linux

---

## Spis treści

- [Podsumowanie](#podsumowanie)
- [Porównanie z Synology Drive Client](#porównanie-z-synology-drive-client)
- [Obecny stan UI — problemy](#obecny-stan-ui--problemy)
  - [Okno i shell aplikacji](#1-okno-i-shell-aplikacji)
  - [Ekran logowania](#2-ekran-logowania)
  - [Strona statusu](#3-strona-statusu)
  - [Strona aktywności](#4-strona-aktywności)
  - [Strona ustawień](#5-strona-ustawień)
  - [System tray](#6-system-tray)
  - [Stylowanie i design system](#7-stylowanie-i-design-system)
  - [Dostępność (a11y)](#8-dostępność-a11y)
- [Rekomendacje UX — wzorowane na Synology](#rekomendacje-ux--wzorowane-na-synology)
- [Propozycja nowej struktury UI](#propozycja-nowej-struktury-ui)
- [Różnice międzyplatformowe](#różnice-międzyplatformowe)
- [Priorytetyzacja zmian](#priorytetyzacja-zmian)

---

## Podsumowanie

Obecny UI to **funkcjonalny prototyp** — minimalna wersja, która pozwala się zalogować i ręcznie zsynchronizować pliki. Wizualnie wygląda jak generyczny Material Design 2 z 2018 roku — biały, niewyróżniający się, bez osobowości marki. W porównaniu z Synology Drive Client brakuje kluczowych wzorców UX oczekiwanych od profesjonalnego klienta sync:

| Cecha | Synology Drive Client | ReadyNextOs Drive |
|-------|----------------------|-------------------|
| Tray popup (główna interakcja) | Kompaktowe okno activity feed | Pełne okno 400×600 |
| Wizard onboardingu | 5-krokowy kreator | Jeden formularz logowania |
| Status per-task | Osobny status każdego zadania sync | Jeden globalny status |
| Selective sync (drzewo folderów) | Checkboxy na drzewie folderów | Brak |
| Przeglądarka folderów | "Browse..." button | Ręczne wpisywanie ścieżek |
| Tray icon — stany wizualne | 3 stany ikony (idle/sync/error) | Jedna statyczna ikona |
| On-demand sync (wirtualne pliki) | Tak (pliki-placeholdery) | Brak |
| Pause/Resume sync | Tak (z tray menu) | Brak |
| Filtr plików (rozmiar, rozszerzenia) | Tak (zakładka File Filter) | `max_file_size_bytes` niezaimplementowany |
| Tryb synchronizacji | Dwukierunkowy / Upload only / Download only | Tylko dwukierunkowy |
| Limit przepustowości | Server-side | Brak |
| Konflikty | Automatyczne renamowanie plików | Tylko rclone `--conflict-resolve=newer` |
| Dark mode | Brak (na desktopie) | Brak |
| Notyfikacje OS | Toast na sync/error/conflict | Plugin zarejestrowany, ale nieużywany |

---

## Obecny stan UI — problemy

### 1. Okno i shell aplikacji

**Plik:** `src-tauri/tauri.conf.json`, `src/styles.css`, `src/App.tsx`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Brak scrollowania** | CRITICAL | `body { overflow: hidden }` — treść poniżej 600px jest ucięta i niedostępna. Activity log z 100 wpisami jest nieczytelny |
| **Sztywny rozmiar 400×600** | HIGH | Brak responsywności, brak `@media` queries. Okno jest resizable ale UI nie adaptuje się do zmiany rozmiaru |
| **Brak loading state** | MEDIUM | "Ładowanie..." jako plain text bez spinnera, bez animacji. Nie wygląda profesjonalnie |
| **Unmount/remount na tab switch** | MEDIUM | Przełączenie tabu resetuje cały state strony — polling startuje od nowa, scroll wraca na górę |
| **Font Inter niezaładowany** | LOW | `font-family: 'Inter', ...` w `index.html` ale font nigdy nie jest pobierany — fallback na system sans-serif |

**Synology robi inaczej:** Dwa tryby okna — kompaktowy tray popup (lekki, activity feed) i pełne okno zarządzania (sidebar + content). ReadyNextOs Drive próbuje zmieścić wszystko w jednym małym oknie.

---

### 2. Ekran logowania

**Plik:** `src/pages/LoginPage.tsx`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Brak logo/ikony** | HIGH | Tylko tekst "ReadyNextOs Drive" 18px — brak ikony aplikacji mimo katalogu `icons/` |
| **Surowe komunikaty błędów** | HIGH | `String(err)` z Rust — użytkownik widzi "invoke error" lub wewnętrzne komunikaty serwera |
| **Brak "Pokaż hasło"** | MEDIUM | Brak ikony oka na polu hasła |
| **Brak walidacji inline** | MEDIUM | Błędy widoczne dopiero po submit, brak per-field validation |
| **Brak autocomplete** | MEDIUM | Brak `autocomplete="username"`, `autocomplete="current-password"` na inputach |
| **Brak SSL checkbox** | MEDIUM | Synology ma "Enable SSL data transmission encryption" — tu brak informacji o bezpieczeństwie połączenia |
| **Jednoetapowy formularz** | LOW | Synology używa 5-krokowego wizarda — mniejsze obciążenie poznawcze |

**Jak wygląda teraz:**
```
┌─────────────────────────────┐
│     ReadyNextOs Drive       │  ← plain text 18px, brak ikony
│   Synchronizacja plików     │
│                             │
│  ┌───────────────────────┐  │
│  │ https://server.com    │  │
│  ├───────────────────────┤  │
│  │ email@example.com     │  │
│  ├───────────────────────┤  │
│  │ ••••••••              │  │
│  ├───────────────────────┤  │
│  │ Błąd: invoke error... │  │  ← surowy error string
│  ├───────────────────────┤  │
│  │   [  Zaloguj się  ]   │  │
│  └───────────────────────┘  │
└─────────────────────────────┘
```

**Jak powinno wyglądać (wzór Synology):**
```
┌─────────────────────────────┐
│        [LOGO ICON]          │
│     ReadyNextOs Drive       │
│                             │
│  Krok 1 z 3: Połączenie     │
│  ┌───────────────────────┐  │
│  │ Adres serwera         │  │
│  │ https://cloud.firm... │  │
│  │ ☑ Szyfrowanie SSL     │  │
│  ├───────────────────────┤  │
│  │       [Dalej →]       │  │
│  └───────────────────────┘  │
│                             │
│  Krok 2: Uwierzytelnianie   │
│  ┌───────────────────────┐  │
│  │ Email                 │  │
│  │ Hasło            [👁]  │  │
│  ├───────────────────────┤  │
│  │    [← Wstecz] [Dalej]│  │
│  └───────────────────────┘  │
│                             │
│  Krok 3: Foldery sync       │
│  ┌───────────────────────┐  │
│  │ Moje pliki: [Browse]  │  │
│  │ Udostępnione: [Browse]│  │
│  ├───────────────────────┤  │
│  │      [Zakończ ✓]      │  │
│  └───────────────────────┘  │
└─────────────────────────────┘
```

---

### 3. Strona statusu

**Plik:** `src/pages/StatusPage.tsx`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Brak szczegółów błędu** | HIGH | Badge "Błąd" bez treści — `status.Error` string jest odrzucany, użytkownik nie wie co się stało |
| **Brak timestamp ostatniej synchronizacji** | HIGH | Nie wiadomo kiedy ostatnio sync się powiódł — kluczowa informacja dla użytkownika |
| **Brak animacji "Syncing"** | MEDIUM | Badge "Synchronizacja" jest statyczny — brak spinnera/pulsowania. Użytkownik nie widzi że coś się dzieje |
| **Emoji jako ikony folderów** | MEDIUM | `📁` / `📂` w produkcyjnej aplikacji desktopowej — niespójne z resztą UI |
| **Brak progress info** | MEDIUM | Podczas sync brak: liczby plików, rozmiaru, procentu postępu |
| **Folder linki niekliklane klawiaturą** | MEDIUM | `<div onClick>` bez `role="button"`, `tabIndex` |
| **Brak Pause/Resume** | LOW | Synology pozwala pauzować sync z poziomu UI i tray menu |

**Jak wygląda teraz:**
```
┌─────────────────────────────┐
│ Status synchronizacji [Idle]│  ← badge bez animacji
│ user@mail · server.com      │
│ [   Synchronizuj teraz   ]  │
│                             │
│ Foldery synchronizacji      │
│ ┌─────────────────────────┐ │
│ │ 📁 Moje pliki           │ │  ← emoji ikona
│ │    ~/ReadyNextOs/Moje.. │ │
│ ├─────────────────────────┤ │
│ │ 📂 Udostępnione         │ │
│ │    ~/ReadyNextOs/Udos.. │ │
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

**Jak powinno wyglądać:**
```
┌─────────────────────────────┐
│ ┌─ Status ────────────────┐ │
│ │ ● Zsynchronizowano      │ │  ← kolorowa kropka + label
│ │   Ostatnia sync: 14:32  │ │  ← timestamp
│ │   Plików: 1,247         │ │  ← statystyki
│ │                         │ │
│ │ [◉ Synchronizuj] [⏸ Pauza]│  ← dwa przyciski
│ └─────────────────────────┘ │
│                             │
│ ┌─ Zadania sync ──────────┐ │
│ │ 🟢 Moje pliki           │ │  ← status dot per folder
│ │    ~/ReadyNextOs/Moje   │ │
│ │    14:32 · 823 pliki    │ │  ← per-task info
│ │    [Otwórz] [Ustawienia]│ │
│ ├─────────────────────────┤ │
│ │ 🟢 Udostępnione         │ │
│ │    ~/ReadyNextOs/Udost  │ │
│ │    14:30 · 424 pliki    │ │
│ │    [Otwórz] [Ustawienia]│ │
│ ├─────────────────────────┤ │
│ │    [+ Dodaj folder]     │ │  ← przyszła funkcjonalność
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

---

### 4. Strona aktywności

**Plik:** `src/pages/ActivityPage.tsx`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Content overflow — ucięte wpisy** | CRITICAL | `body { overflow: hidden }` + brak scroll containera = dolne wpisy są **permanentnie niedostępne** |
| **Długie ścieżki plików overflow** | HIGH | Brak `text-overflow: ellipsis` — ścieżki wychodzą poza kontener 400px |
| **Brak filtrowania** | MEDIUM | Brak możliwości filtrowania po: typie akcji, statusie, ścieżce |
| **Brak manualnego odświeżenia** | MEDIUM | Tylko auto-poll co 10s, brak przycisku "Odśwież" |
| **Brak grupowania** | LOW | Flat lista — brak grupowania po dniu/godzinie jak w Synology |
| **React key={i} antipattern** | LOW | Index jako key — problem przy dynamicznej liście |

**Jak wygląda teraz:**
```
┌─────────────────────────────┐
│ Ostatnia aktywność          │
│ ┌─────────────────────────┐ │
│ │ Sync zakończony  [✓ ok] │ │
│ │ /very/long/path/to/fi.. │ │  ← brak ellipsis
│ │ 14:32:05                │ │
│ ├─────────────────────────┤ │
│ │ Pobrano plik    [✓ ok]  │ │
│ │ /another/path/file.txt  │ │
│ │ 14:31:58                │ │
│ ├ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤ │
│ │    ... reszta ucięta    │ │  ← overflow: hidden
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

**Jak powinno wyglądać:**
```
┌─────────────────────────────┐
│ Aktywność     [🔄] [Filtr ▾]│  ← refresh + filter
│ ┌─────────────────────────┐ │
│ │ Dzisiaj, 14:32          │ │  ← grupowanie po dniu
│ │                         │ │
│ │ ↑ Wysłano  report.pdf   │ │  ← ikona kierunku
│ │   Moje pliki · 2.4 MB  │ │  ← folder + rozmiar
│ │                         │ │
│ │ ↓ Pobrano  notes.txt    │ │
│ │   Udostępnione · 12 KB │ │
│ │                         │ │
│ │ ⚠ Konflikt  budget.xlsx │ │  ← wyróżnione konflikty
│ │   Zachowano nowszą wersję│ │
│ ├─────────────────────────┤ │
│ │ Wczoraj, 18:15          │ │
│ │ ...                     │ │
│ │         ← scrollowalne →│ │  ← scroll container!
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

---

### 5. Strona ustawień

**Plik:** `src/pages/SettingsPage.tsx`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Brak "Browse..." dla ścieżek** | HIGH | Użytkownik musi ręcznie wpisywać pełne ścieżki katalogów — niedopuszczalne w desktopowej aplikacji |
| **Przycisk Wyloguj tuż pod Zapisz** | HIGH | 8px marginesu między "Zapisz ustawienia" a czerwonym "Wyloguj" — ryzyko przypadkowego wylogowania |
| **Natywne checkboxy bez stylizacji** | MEDIUM | Systemowe checkboxy wyglądają obco w kontekście card-based UI |
| **Brak loadera / error state** | MEDIUM | `if (!config) return null` — biały ekran podczas ładowania |
| **Brak potwierdzenia wylogowania** | MEDIUM | Kliknięcie "Wyloguj" natychmiast czyści sesję bez dialogu potwierdzenia |
| **Wiadomość "Zapisano" bez timeout** | LOW | Komunikat sukcesu pozostaje na ekranie na zawsze |
| **Brak sekcji "Filtr plików"** | LOW | Synology ma zakładkę z filtrem rozmiaru, rozszerzeń, blacklistą nazw |
| **`max_file_size_bytes` ukryty** | LOW | Pole istnieje w configu ale nie jest wyświetlane w UI |

**Jak wygląda teraz:**
```
┌─────────────────────────────┐
│ ┌─ Synchronizacja ────────┐ │
│ │ Interwał: [300    ] sek │ │
│ │ ☐ Obserwuj zmiany lokalne│ │  ← natywny checkbox
│ │ ☐ Synchronizuj przy star │ │
│ └─────────────────────────┘ │
│ ┌─ Ścieżki ───────────────┐ │
│ │ Moje pliki:              │ │
│ │ [~/ReadyNextOs/Moje...] │ │  ← ręczne wpisywanie
│ │ Udostępnione:            │ │
│ │ [~/ReadyNextOs/Udos...] │ │  ← brak "Browse..."
│ └─────────────────────────┘ │
│ ┌─ Konto ─────────────────┐ │
│ │ user@mail.com            │ │
│ │ server.readynextos.com   │ │
│ │                          │ │
│ │ [ Zapisz ustawienia ]    │ │
│ │ [     Wyloguj       ]    │ │  ← 8px od "Zapisz"!
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

**Jak powinno wyglądać:**
```
┌─────────────────────────────┐
│ ┌─ Synchronizacja ────────┐ │
│ │ Interwał:                │ │
│ │ [300] sekund  (30-3600)  │ │  ← jednostka + zakres
│ │                          │ │
│ │ ○─── Obserwuj zmiany     │ │  ← custom toggle switch
│ │ ○─── Sync przy starcie   │ │
│ └─────────────────────────┘ │
│ ┌─ Foldery ───────────────┐ │
│ │ Moje pliki:              │ │
│ │ [~/ReadyNextOs/Mo...] [📂]│ │  ← Browse button!
│ │ Udostępnione:            │ │
│ │ [~/ReadyNextOs/Ud...] [📂]│ │
│ └─────────────────────────┘ │
│ ┌─ Filtr plików ──────────┐ │
│ │ Max rozmiar: [100] MB    │ │  ← nowa sekcja
│ │ Pomijaj: .tmp .lnk .DS_ │ │
│ └─────────────────────────┘ │
│ ┌─ Konto ─────────────────┐ │
│ │ 👤 user@mail.com         │ │
│ │ 🖥 server.readynextos.com│ │
│ │                          │ │
│ │ [ Zapisz ustawienia ✓ ] │ │
│ │                          │ │
│ │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─  │ │  ← separator wizualny
│ │                          │ │
│ │ [  Wyloguj  ]  ← osobna │ │  ← wyraźne oddzielenie
│ │              sekcja      │ │
│ └─────────────────────────┘ │
│                             │
│ ┌─ Informacje ────────────┐ │
│ │ Wersja: 0.2.0            │ │  ← nowa sekcja
│ │ rclone: 1.68.2           │ │
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

---

### 6. System tray

**Plik:** `src-tauri/src/main.rs` (tray setup)

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Jedna statyczna ikona tray** | HIGH | Brak wizualnego rozróżnienia stanów idle/syncing/error w ikonie |
| **Minimalne tray menu** | MEDIUM | Tylko "Pokaż okno" + "Zakończ" — brak Pause, Open folder, Status |
| **Brak tray popup** | MEDIUM | Synology: left-click = kompaktowy popup z activity. Tu: left-click = pełne okno |
| **Brak tooltipa dynamicznego** | LOW | Tooltip jest statyczny "ReadyNextOs Drive" — nie pokazuje aktualnego statusu |

**Jak wygląda teraz:**
```
Tray icon (statyczny) → Right-click:
┌──────────────┐
│ Pokaż okno   │
│ Zakończ      │
└──────────────┘
```

**Jak powinno wyglądać (wzór Synology):**
```
Tray icon (zmienia się wg stanu):
  🟢 idle | 🔄 syncing (animowany) | 🔴 error

Left-click → Kompaktowy popup:
┌──────────────────────────┐
│ ● Zsynchronizowano 14:32 │
│ ─────────────────────────│
│ ↑ report.pdf     2 min   │
│ ↓ notes.txt      5 min   │
│ ↑ image.png     12 min   │
│ ─────────────────────────│
│ [Otwórz folder] [⚙]     │
└──────────────────────────┘

Right-click:
┌──────────────────────────┐
│ ● Zsynchronizowano       │
│ ─────────────────────────│
│ Otwórz Moje pliki        │
│ Otwórz Udostępnione      │
│ ─────────────────────────│
│ ⏸ Wstrzymaj sync         │
│ 🔄 Synchronizuj teraz    │
│ ─────────────────────────│
│ ⚙ Ustawienia             │
│ ─────────────────────────│
│ Zakończ                  │
└──────────────────────────┘
```

---

### 7. Stylowanie i design system

**Plik:** `src/styles.css`

| Problem | Severity | Szczegóły |
|---------|----------|-----------|
| **Brak dark mode** | HIGH | Brak `@media (prefers-color-scheme: dark)`. Jasny UI na ciemnym systemie jest rażący |
| **`user-select: none` na body** | HIGH | Użytkownik nie może kopiować tekstu — ścieżek, błędów, emaili |
| **Generyczny wygląd** | MEDIUM | Klon Material Design 2 bez brandingu — wygląda jak tutorial app |
| **Inline styles w komponentach** | MEDIUM | `style={{ fontSize: 14, marginBottom: 12 }}` zamiast CSS klas — utrudnia theming |
| **Nieużywane CSS tokeny** | LOW | `--color-primary-light`, `--color-bg` zadeklarowane ale nigdy nieużywane |
| **Nieużywana klasa `.btn-outline`** | LOW | Zdefiniowana w CSS, nigdzie nie stosowana |
| **Nieużywane zależności** | LOW | `lucide-react` i `react-router-dom` w `package.json` ale nigdzie nie importowane |

**Rekomendacja design system:**

Zamiast ręcznego CSS warto rozważyć:
- **Opcja A:** Tailwind CSS — szybkie prototypowanie, dobry dark mode, brak vendor lock-in
- **Opcja B:** Radix UI + Tailwind — gotowe, dostępne komponenty (dialog, dropdown, tabs, toggle) z pełnym a11y
- **Opcja C:** shadcn/ui — gotowe komponenty Radix + Tailwind, kopiowane do projektu (nie dependency)

Każda z tych opcji rozwiązuje jednocześnie: dark mode, a11y, custom checkboxy, scroll containery, responsive design.

---

### 8. Dostępność (a11y)

| Problem | Element | Szczegóły |
|---------|---------|-----------|
| **Brak keyboard navigation** | Nav tabs (`App.tsx:52-69`) | `<div onClick>` bez `role="tab"`, `tabIndex`, `onKeyDown` |
| **Brak focus visible** | Wszystkie inputy (`styles.css:83`) | `outline: none` bez zamiennika — klawiaturowy użytkownik nie widzi focusa |
| **Niekliklane folder linki** | `StatusPage.tsx:74,84` | `<div onClick>` bez `role="button"`, `tabIndex` |
| **Brak ARIA** | Cały UI | Brak `role="tablist"`, `role="tabpanel"`, `aria-selected`, `role="status"` |
| **Brak screen reader info** | Status badge | Status synchronizacji nie jest `role="status"` ani `aria-live` |
| **Brak label na checkbox** | `SettingsPage.tsx:64-85` | Checkboxy mają `<label>` ale brak `id`/`htmlFor` powiązania |
| **Kontrast kolorów** | `--color-text-secondary` | `rgba(0,0,0,0.6)` na `#fafafa` — kontrast 5.7:1, minimalnie przechodzi WCAG AA |

---

## Rekomendacje UX — wzorowane na Synology

### R1. Dwa tryby okna: Tray Popup + Pełne okno

**Synology pattern:** Left-click na tray = kompaktowy popup z activity feed. Double-click = pełne okno zarządzania z sidebar.

**Propozycja:**
- **Tray popup (primary):** Kompaktowe okno ~350×400px, activity feed + status + szybkie akcje
- **Pełne okno (secondary):** Otwierane z popup lub double-click tray, ~700×500px, sidebar z zakładkami

To wymaga zmian w Tauri — dwa okna z różną konfiguracją. Tauri v2 obsługuje multi-window.

---

### R2. Wizard onboardingu zamiast jednego formularza

**Synology pattern:** 5-krokowy kreator (welcome → connection → auth → task type → folder config → done).

**Propozycja minimalna (3 kroki):**
1. **Połączenie** — adres serwera + checkbox SSL
2. **Logowanie** — email + hasło
3. **Foldery** — wybór katalogów synchronizacji z "Browse..."

Implementacja: `LoginPage.tsx` → `SetupWizard.tsx` z `useState<step>` i animowanymi przejściami.

---

### R3. Dynamiczne stany ikony tray

**Synology pattern:** Ikona tray zmienia się wizualnie:
- Zielony checkmark = idle/synced
- Niebieski spinner = syncing
- Czerwony wykrzyknik = error

**Implementacja w Tauri:**
```rust
// Zmiana ikony tray na podstawie statusu
tray.set_icon(match status {
    SyncStatus::Idle => Some(Icon::from_path("icons/tray-idle.png")),
    SyncStatus::Syncing => Some(Icon::from_path("icons/tray-syncing.png")),
    SyncStatus::Error(_) => Some(Icon::from_path("icons/tray-error.png")),
    _ => Some(Icon::from_path("icons/tray-icon.png")),
})?;
```

Wymaga przygotowania 3-4 wariantów ikony tray.

---

### R4. Rozbudowane tray menu

**Obecne:** 2 pozycje (Pokaż okno, Zakończ).

**Propozycja:**
```
● Zsynchronizowano (14:32)     ← status z timestampem (disabled, info only)
───────────────────────
Otwórz Moje pliki              ← skrót do folderu
Otwórz Udostępnione
───────────────────────
⏸ Wstrzymaj synchronizację     ← pause/resume toggle
🔄 Synchronizuj teraz
───────────────────────
⚙ Ustawienia...               ← otwiera pełne okno na zakładce Settings
───────────────────────
Zakończ
```

---

### R5. Dialog "Browse..." dla wyboru folderów

**Krytyczne dla UX** — ręczne wpisywanie ścieżek to antypattern w aplikacji desktopowej.

**Implementacja w Tauri:**
```rust
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog()
        .file()
        .set_title("Wybierz folder synchronizacji")
        .blocking_pick_folder();
    Ok(folder.map(|f| f.to_string_lossy().to_string()))
}
```

Wymaga dodania `tauri-plugin-dialog` do zależności.

---

### R6. Scroll container na stronach z dużą ilością treści

```css
.page-content {
    overflow-y: auto;
    max-height: calc(100vh - 60px); /* odejmij nawigację */
    scrollbar-width: thin;
    scrollbar-color: #ccc transparent;
}
```

---

### R7. Dark mode

```css
@media (prefers-color-scheme: dark) {
    :root {
        --color-bg: #1a1a2e;
        --color-surface: #16213e;
        --color-text: rgba(255, 255, 255, 0.87);
        --color-text-secondary: rgba(255, 255, 255, 0.6);
        --color-primary: #64b5f6;
        --shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    }

    body { background: var(--color-bg); color: var(--color-text); }
    .card { background: var(--color-surface); }
    .input { background: var(--color-bg); color: var(--color-text); border-color: #333; }
}
```

---

### R8. Notyfikacje systemowe

Plugin `tauri-plugin-notification` jest zarejestrowany ale nigdzie nie używany.

**Kiedy notyfikować:**
- Sync zakończony (szczególnie po długim syncu)
- Błąd synchronizacji
- Konflikt plików
- Token wygasł / wymagane ponowne logowanie

---

## Propozycja nowej struktury UI

### Architektura okien

```
┌─────────────────────────────────────────────┐
│                SYSTEM TRAY                   │
│  [Ikona] → Left-click: Tray Popup          │
│           → Right-click: Context Menu       │
│           → Double-click: Full Window       │
└──────┬──────────────────────────┬───────────┘
       │                          │
       ▼                          ▼
┌──────────────┐    ┌────────────────────────────┐
│  Tray Popup  │    │       Full Window           │
│  350 × 400   │    │       700 × 500             │
│              │    │                              │
│ ┌──────────┐ │    │ ┌────────┐ ┌──────────────┐ │
│ │● Status  │ │    │ │Sidebar │ │   Content    │ │
│ │  14:32   │ │    │ │        │ │              │ │
│ ├──────────┤ │    │ │ Status │ │  (zależne    │ │
│ │Activity  │ │    │ │ Active │ │   od wyboru  │ │
│ │feed      │ │    │ │ Ustawien│ │   w sidebar) │ │
│ │(scroll)  │ │    │ │ O aplik│ │              │ │
│ ├──────────┤ │    │ │        │ │              │ │
│ │[Folder]  │ │    │ │        │ │              │ │
│ │[⚙][Sync] │ │    │ │        │ │              │ │
│ └──────────┘ │    │ └────────┘ └──────────────┘ │
└──────────────┘    └────────────────────────────┘
```

### Nawigacja Full Window — sidebar

```
┌────────────────────┐
│ ☁ ReadyNextOs Drive │
│ ─────────────────── │
│                     │
│ 📊 Status           │  ← overview + sync tasks
│ 📋 Aktywność        │  ← activity log z filtrem
│ ⚙ Ustawienia        │  ← sync config + folders
│ ℹ O aplikacji       │  ← wersja, licencja, help
│                     │
│ ─────────────────── │
│ 👤 user@mail.com    │
│    [Wyloguj]        │  ← bezpieczne oddzielenie
└────────────────────┘
```

---

## Różnice międzyplatformowe

### Windows
- Tray w notification area (prawy dolny róg)
- Natywne dekoracje okna (title bar z min/max/close)
- Integracja z Explorer (context menu, overlay badges) — możliwe przez Windows Shell Extension (poza Tauri, ale przyszłościowo)
- `.msi` installer z opcjami: autostart, lokalizacja instalacji
- Font: Segoe UI (system-ui fallback)

### macOS
- Menu bar icon (prawy górny róg)
- Natywne dekoracje z traffic lights (red/yellow/green)
- Finder sidebar integration — sync foldery w Locations
- `.dmg` z drag-to-Applications
- Font: SF Pro (system-ui fallback)
- Respektowanie `NSAppSleepDisabled` — sync musi działać gdy lid closed

### Linux
- System tray (AppIndicator na GNOME, system tray na KDE/XFCE)
- **Znany problem Synology:** tray icon często nie wyświetla się na wielu DE — ReadyNextOs Drive powinien obsługiwać fallback (standalone window mode)
- `libappindicator3` wymagany — już jest w prereqs
- AppImage jako uniwersalny format
- Font: system sans-serif (Cantarell na GNOME, Noto Sans na KDE)
- **WebKitGTK workaround** — już zaimplementowany w `main.rs` (EGL rendering fix)

### Tablica zgodności

| Funkcja | Windows | macOS | Linux |
|---------|---------|-------|-------|
| Tray icon | ✅ Native | ✅ Menu bar | ⚠ AppIndicator (nie wszystkie DE) |
| Tray popup | ✅ | ✅ | ⚠ Ograniczone na Wayland |
| Natywne dekoracje | ✅ | ✅ | ✅ (ale różne per WM) |
| File dialog (Browse) | ✅ | ✅ | ✅ (GTK) |
| Notyfikacje | ✅ Toast | ✅ Notification Center | ✅ libnotify |
| Autostart | ✅ Registry | ✅ LaunchAgent | ✅ XDG autostart |
| Dark mode detect | ✅ | ✅ | ⚠ Zależy od DE |
| Overlay badges na plikach | Możliwe (Shell Ext) | Możliwe (Finder Ext) | Brak standardu |

---

## Priorytetyzacja zmian

### Faza 1 — Fundament (naprawienie obecnych błędów UX)

| # | Zmiana | Wysiłek | Wpływ |
|---|--------|---------|-------|
| 1 | Włączyć scroll na stronach (`overflow-y: auto`) | Niski | CRITICAL — treść jest ucięta |
| 2 | Usunąć `user-select: none` z body | Niski | HIGH — tekst nie do skopiowania |
| 3 | Wyświetlać szczegóły błędu z `SyncStatus.Error` | Niski | HIGH |
| 4 | Dodać timestamp ostatniej synchronizacji | Niski | HIGH |
| 5 | Dodać `text-overflow: ellipsis` na ścieżkach | Niski | HIGH |
| 6 | Oddzielić przycisk Wyloguj od Zapisz (separator + confirm dialog) | Niski | HIGH |
| 7 | User-friendly komunikaty błędów logowania | Niski | HIGH |
| 8 | Dodać animację/spinner na statusie "Syncing" | Niski | MEDIUM |
| 9 | Załadować font Inter (lub usunąć z font-family) | Niski | LOW |
| 10 | Usunąć nieużywane deps (lucide-react, react-router-dom) | Niski | LOW |

### Faza 2 — Profesjonalizacja

| # | Zmiana | Wysiłek | Wpływ |
|---|--------|---------|-------|
| 11 | Dark mode (`prefers-color-scheme`) | Średni | HIGH |
| 12 | Dialog "Browse..." na ścieżki folderów (`tauri-plugin-dialog`) | Średni | HIGH |
| 13 | Rozbudowane tray menu (status, open folder, pause, sync now) | Średni | HIGH |
| 14 | Dynamiczne ikony tray (idle/syncing/error) | Średni | HIGH |
| 15 | Custom checkboxy / toggle switches | Niski | MEDIUM |
| 16 | Keyboard navigation + ARIA na tabach i linkach | Średni | MEDIUM |
| 17 | Notyfikacje systemowe (sync done, error, conflict) | Średni | MEDIUM |
| 18 | Logo/ikona na ekranie logowania | Niski | MEDIUM |
| 19 | Focus visible styles (zamiennik `outline: none`) | Niski | MEDIUM |

### Faza 3 — Parytet z Synology

| # | Zmiana | Wysiłek | Wpływ |
|---|--------|---------|-------|
| 20 | Wizard onboardingu (3 kroki) zamiast jednego formularza | Średni | HIGH |
| 21 | Tray popup (kompaktowe okno activity) — Tauri multi-window | Wysoki | HIGH |
| 22 | Pełne okno z sidebar layout (~700×500) | Wysoki | HIGH |
| 23 | Activity log z filtrami i grupowaniem | Średni | MEDIUM |
| 24 | Per-task status (osobny status/statystyki per folder sync) | Średni | MEDIUM |
| 25 | Filtr plików UI (max size, extension blacklist) | Średni | MEDIUM |
| 26 | Pause/Resume synchronizacji | Średni | MEDIUM |
| 27 | Wersja aplikacji + info w Settings | Niski | LOW |
| 28 | Selective sync (drzewo folderów z checkboxami) | Wysoki | MEDIUM |

### Faza 4 — Przewaga nad Synology

| # | Zmiana | Wysiłek | Wpływ |
|---|--------|---------|-------|
| 29 | Dark mode (Synology nie ma na desktopie) | Już w Fazie 2 | Competitive advantage |
| 30 | Solidna obsługa Linux (tray fallback, Wayland) | Średni | Competitive advantage |
| 31 | Aktywne rozwiązywanie konfliktów (UI dialog zamiast auto-rename) | Wysoki | Competitive advantage |
| 32 | Client-side bandwidth throttling | Średni | Competitive advantage |
| 33 | Wyszukiwanie w activity log | Niski | Quality of life |
