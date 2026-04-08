# ReadyNextOs Drive — Review Techniczny

**Data:** 2026-04-08
**Wersja:** 0.2.0
**Zakres:** przegląd statyczny kodu Rust + React bez uruchamiania aplikacji i bez testów runtime

---

## Cel dokumentu

Ten dokument rozdziela cztery różne klasy problemów:

1. **Potwierdzone defekty** — problemy bezpośrednio widoczne w kodzie.
2. **Ryzyka / hipotezy** — istotne zagrożenia, ale nie w pełni dowiedzione samą lekturą kodu.
3. **Brakujące funkcjonalności** — elementy obecne w UI/config lub sugerowane przez architekturę, ale niezaimplementowane.
4. **Usprawnienia techniczne** — refaktor, a11y, UX, performance, cleanup.

To nie jest audyt „pełny”. To jest **review statyczne**, więc nie potwierdza zachowania aplikacji w runtime, nie zastępuje testów i nie dowodzi exploitowalności wszystkich scenariuszy.

---

## Werdykt

Kod wygląda jak **działający prototyp**: podstawowy przepływ logowania, odczytu statusu, ręcznego syncu i ustawień jest spójny, ale projekt ma:

- kilka **potwierdzonych defektów wysokiego priorytetu**,
- kilka **poważnych ryzyk bezpieczeństwa**,
- wyraźne **braki funkcjonalne** między UI/config a backendem,
- oraz grupę **usprawnień technicznych**, których nie należy mieszać z bugfixami.

**Rekomendacja:** przed releasem naprawić wszystkie pozycje z sekcji `P1` i `P2`.

---

## Podsumowanie kategorii

| Kategoria | Liczba | Uwagi |
|-----------|--------|-------|
| Potwierdzone defekty | 8 | Bezpośrednio widoczne w kodzie |
| Ryzyka / hipotezy | 5 | Wysoka istotność, ale nie wszystkie są twardo dowiedzione |
| Brakujące funkcjonalności | 6 | UI/config obiecują więcej niż backend realizuje |
| Usprawnienia techniczne | 9 | Refaktor, UX, a11y, observability |

---

## P1 — Potwierdzone defekty krytyczne i wysokie

### D1. `sync_all()` zwraca sukces nawet po błędzie synchronizacji

**Plik:** `src-tauri/src/sync.rs:22-91`
**Status:** potwierdzony defekt
**Priorytet:** P1

`sync_all()` ustawia status błędu wewnętrznie, ale na końcu zawsze zwraca `Ok(())`. W efekcie `trigger_sync()` może zwrócić sukces do frontendu mimo nieudanego syncu.

**Skutek:** błędy synchronizacji są częściowo ukrywane przed callerem.

---

### D2. Token trafia do argumentów procesu przy `rclone obscure`

**Plik:** `src-tauri/src/sync.rs:168-183`
**Status:** potwierdzony defekt bezpieczeństwa
**Priorytet:** P1

Kod używa:

```rust
.args(["obscure", password])
```

To oznacza, że token pojawia się w argumentach procesu potomnego. Na systemach uniksowych może to być widoczne dla innych procesów przez listę procesów lub `/proc`.

**Skutek:** niepotrzebny wyciek sekretu poza keychain i env.

---

### D3. Frontend ma zbyt szerokie uprawnienia shell

**Pliki:** `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
**Status:** potwierdzona zła konfiguracja
**Priorytet:** P1

Potwierdzone elementy:

- `csp` jest ustawione na `null`,
- capability zawiera `shell:allow-execute`,
- capability zawiera `shell:allow-spawn`,
- capability zawiera `shell:allow-kill`,
- capability zawiera `shell:allow-stdin-write`.

To jest realny problem konfiguracyjny. Sam dokument nie powinien jednak twierdzić, że istniejący exploit XSS już został znaleziony, bo tego przegląd nie dowodzi.

**Wniosek poprawny:** profil bezpieczeństwa jest zbyt szeroki.

---

### D4. `open_folder()` otwiera dowolną ścieżkę/URL z frontendu

**Plik:** `src-tauri/src/main.rs:124-128`
**Status:** potwierdzony defekt bezpieczeństwa
**Priorytet:** P1

`open::that(&path)` jest wywoływane bez walidacji źródła i bez ograniczenia do katalogów synchronizacji.

**Skutek:** backend ufa dowolnemu parametrowi przekazanemu przez frontend.

---

### D5. Brak walidacji `server_url` przy logowaniu i zapisie configu

**Pliki:** `src-tauri/src/auth.rs:83-112`, `src-tauri/src/main.rs:91-100`
**Status:** potwierdzony defekt walidacji
**Priorytet:** P1

`server_url` jest używane bez walidacji schematu i hosta. `update_config()` przyjmuje cały `AppConfig` z frontendu i zapisuje go bez żadnych reguł backendowych.

**Co jest pewne:** brak walidacji.
**Czego ten review nie dowodzi:** pełnego scenariusza SSRF/exfiltracji w każdej konfiguracji środowiska.

---

### D6. `FileWatcher` jest martwym kodem

**Pliki:** `src-tauri/src/watcher.rs`, `src-tauri/src/main.rs:159-178`
**Status:** potwierdzony problem implementacyjny
**Priorytet:** P1

Watcher jest tworzony i przechowywany w stanie aplikacji, ale:

- `start()` nie jest nigdzie wywoływane,
- `has_changes()` nie jest nigdzie konsumowane.

**Skutek:** subsystem istnieje w kodzie, ale nie działa.

---

### D7. Ustawienia syncu istnieją w UI/config, ale backend ich nie używa

**Pliki:** `src-tauri/src/config.rs`, `src/pages/SettingsPage.tsx`, `src-tauri/src/main.rs`, `src-tauri/src/sync.rs`
**Status:** potwierdzony brak spięcia implementacji
**Priorytet:** P1

Dotyczy pól:

- `sync_interval_secs`
- `watch_local_changes`
- `sync_on_startup`
- `max_file_size_bytes`

Pola są zapisane w configu i edytowalne z UI, ale nie widać backendowej logiki, która używa ich do schedulerów, watchera, startup sync albo ograniczeń `rclone`.

---

### D8. `SettingsPage` pozwala zapisać niewalidowane wartości liczbowe i ścieżki

**Plik:** `src/pages/SettingsPage.tsx:50-110`
**Status:** potwierdzony defekt frontendowej walidacji
**Priorytet:** P2

Przykłady:

- `parseInt(e.target.value) || 300`
- brak clamp po stronie JS,
- ścieżki przyjmowane jako dowolny tekst.

**Skutek:** UI zapisuje wartości niezgodne z własnymi założeniami, a backend ich nie broni.

---

## P2 — Potwierdzone defekty średnie

### D9. `reqwest::Client` nie ma timeoutu

**Plik:** `src-tauri/src/auth.rs:88`
**Status:** potwierdzony defekt niezawodności
**Priorytet:** P2

Login może wisieć zbyt długo lub bez sensownego limitu czasu.

---

### D10. `rclone` uruchamiany bez timeoutu

**Plik:** `src-tauri/src/sync.rs:126-136`
**Status:** potwierdzony defekt niezawodności
**Priorytet:** P2

`.output().await` czeka bez limitu czasu.

---

### D11. Błędy odpowiedzi serwera są zwracane surowo do UI

**Plik:** `src-tauri/src/auth.rs:102-105`
**Status:** potwierdzony defekt obsługi błędów
**Priorytet:** P2

Kod zwraca:

```rust
Err(format!("Login failed ({}): {}", status, body))
```

To może ujawniać użytkownikowi zbyt dużo szczegółów backendu.

---

### D12. `SettingsPage` renderuje pusty ekran podczas ładowania

**Plik:** `src/pages/SettingsPage.tsx:42`
**Status:** potwierdzony defekt UX
**Priorytet:** P2

`if (!config) return null;` oznacza brak loading state i brak error state.

---

## Ryzyka / hipotezy wymagające ostrożniejszego sformułowania

To są ważne problemy, ale nie należy ich opisywać tak, jakby review już udowodnił pełny exploit lub reprodukowalny błąd.

### R1. XSS + shell permissions może prowadzić do RCE

**Status:** ryzyko wysokie

Przy `csp: null` i bardzo szerokich shell permissions potencjalny XSS byłby bardzo groźny. To jest prawidłowy model zagrożenia.

**Czego ten review nie potwierdza:** że w aktualnym kodzie istnieje już konkretny wektor XSS gotowy do wykorzystania.

---

### R2. `.lock().unwrap()` zwiększa ryzyko kaskadowego crasha po poisoningu

**Pliki:** `src-tauri/src/main.rs`, `src-tauri/src/sync.rs`
**Status:** ryzyko wysokie

To jest sensowna uwaga o odporności systemu. Nie jest jednak poprawne twierdzić bezwarunkowo, że aplikacja już teraz „crashuje od mutex poisoning”, jeśli review nie pokazuje panic path, który poison wywołuje.

---

### R3. `logout()` ma słaby lock ordering i TOCTOU

**Plik:** `src-tauri/src/main.rs:60-74`
**Status:** ryzyko średnie

Kod bierze kilka locków w nieoptymalnej kolejności i sięga wielokrotnie po `config`. To uzasadnia refaktor.

**Czego review nie dowodzi:** deterministycznego deadlocka.

---

### R4. Brak concurrent sync guard grozi wyścigiem

**Plik:** `src-tauri/src/sync.rs:22-32`
**Status:** ryzyko wysokie

`sync_all()` nie blokuje drugiego uruchomienia, gdy status jest już `Syncing`.

To jest ważne ryzyko, ale bez testu runtime review nie pokazuje, jak Tauri i UI zachowają się podwójnego kliknięcia w praktyce.

---

### R5. `open_folder()` i brak walidacji ścieżek powiększają powierzchnię ataku

**Status:** ryzyko wysokie

To jest poprawny wniosek bezpieczeństwa. Nie należy go jednak opisywać jako gotowego chainu exploit bez rozdzielenia:

- potwierdzonego braku walidacji,
- odrębnego założenia o przejęciu frontendu lub złośliwym wejściu.

---

## Brakujące funkcjonalności

To nie są bugfixy. To są luki między obietnicą produktu a implementacją.

### F1. Brak background sync schedulera

`sync_interval_secs` istnieje, ale nie ma scheduler loop ani timera backendowego.

### F2. Brak integracji file watchera

Watcher istnieje jako moduł, ale nie bierze udziału w działaniu aplikacji.

### F3. Brak `sync_on_startup`

Opcja jest w configu i UI, ale nie widać logiki startowej, która ją respektuje.

### F4. Brak użycia `max_file_size_bytes`

Opcja istnieje, ale nie jest przekazywana do `rclone`.

### F5. Brak eventów Tauri dla statusu syncu

Frontend opiera się na pollingu zamiast event-driven status updates.

### F6. Brak retry/backoff i anulowania synchronizacji

To nie musi być wymagane dla MVP, ale jest brakującym elementem klienta synchronizującego.

---

## Usprawnienia techniczne

To nie są defekty blokujące release, ale sensowne prace porządkowe.

### U1. Frontend polling może powodować nakładanie requestów

`StatusPage` i `ActivityPage` odświeżają się interwałami bez prostego guardu `inFlight`.

### U2. `ActivityPage` używa indeksu jako React key

To pogarsza stabilność renderu po odwróceniu listy.

### U3. Taby w `App.tsx` są oparte o `<div>`

To obniża dostępność klawiaturową i semantykę.

### U4. `console.error` w UI nie daje sensownego telemetry/logging w aplikacji desktopowej

Przydałby się normalny logger lub wyciszenie części błędów.

### U5. `serde_json::to_string(...).unwrap()` w `login()`

To mały, ale zbędny panic point.

### U6. `tokio` z `features = ["full"]`

Warto zawęzić, jeśli nie ma realnej potrzeby pełnego pakietu.

### U7. Nieużywane zależności npm

`react-router-dom` i `lucide-react` wyglądają na nieużywane.

### U8. Globalne `user-select: none`

Jeśli faktycznie jest ustawione globalnie, pogarsza użyteczność przy kopiowaniu błędów i ścieżek.

### U9. Architektura stanu i błędów jest uproszczona

- `Result<T, String>` wszędzie,
- brak typed errors,
- bezpośredni dostęp do `sync_engine.status`,
- podwójne `Arc/Mutex`.

To są raczej kandydaci do refaktoru niż pilne bugi.

---

## Priorytety napraw

### Przed releasem

1. Naprawić zwracanie błędów z `sync_all()`.
2. Usunąć token z argumentów procesu.
3. Ograniczyć shell permissions i przywrócić sensowny CSP.
4. Dodać walidację `server_url` i backendową walidację `AppConfig`.
5. Ograniczyć `open_folder()` do dozwolonych katalogów.
6. Zdecydować: albo wdrożyć watcher/scheduler, albo usunąć martwe ustawienia z UI.

### W następnej iteracji

1. Dodać concurrent sync guard.
2. Dodać timeouty dla HTTP i `rclone`.
3. Dodać loading/error states w ustawieniach.
4. Oczyścić frontend polling i a11y.
5. Uporządkować błędy i logging.

---

## Checklista

### P1 — przed releasem

- [ ] `sync_all()` ma zwracać `Err(...)` przy nieudanej synchronizacji.
- [ ] Usunąć token z argumentów procesu `rclone obscure`.
- [ ] Włączyć sensowny CSP w konfiguracji Tauri.
- [ ] Usunąć zbędne frontendowe uprawnienia `shell:*`.
- [ ] Dodać backendową walidację `server_url`.
- [ ] Dodać backendową walidację `AppConfig` w `update_config()`.
- [ ] Ograniczyć `open_folder()` do katalogów synchronizacji.
- [ ] Zdecydować: wdrożyć watcher/scheduler albo usunąć martwe opcje z UI.

### P2 — następna iteracja

- [ ] Dodać guard przed równoległym uruchomieniem synchronizacji.
- [ ] Dodać timeout dla `reqwest::Client`.
- [ ] Dodać timeout dla procesu `rclone`.
- [ ] Nie zwracać surowego body błędu serwera do UI.
- [ ] Dodać loading/error state w `SettingsPage`.
- [ ] Poprawić walidację pól liczbowych i ścieżek w ustawieniach.
- [ ] Ograniczyć nakładanie się requestów z pollingu.
- [ ] Poprawić a11y nawigacji i elementów otwierających foldery.
- [ ] Zastąpić `console.error` sensownym logowaniem desktopowym.

### P3 — porządki techniczne

- [ ] Rozważyć typed errors zamiast `Result<T, String>`.
- [ ] Ograniczyć użycie `.lock().unwrap()` albo obsłużyć poisoning.
- [ ] Uporządkować dostęp do stanu `sync_engine.status`.
- [ ] Usunąć nieużywane zależności npm.
- [ ] Ograniczyć feature set `tokio`, jeśli nie jest potrzebny pełny pakiet.
- [ ] Usunąć drobne panic points typu `unwrap()` w ścieżkach IPC.

---

## Czego ten review nie obejmuje

- testów manualnych,
- testów integracyjnych z serwerem WebDAV,
- potwierdzenia exploitowalności XSS/RCE,
- oceny zależności pod kątem CVE,
- oceny zachowania tray/app lifecycle na różnych systemach.

---

## Wniosek końcowy

Poprzednia wersja review mieszała:

- twarde defekty,
- ryzyka,
- brakujące funkcje,
- oraz sugestie architektoniczne.

Po uporządkowaniu obraz jest prostszy:

- są **realne problemy wysokiego priorytetu**, które trzeba naprawić,
- ale nie wszystkie mocne tezy były wcześniej udowodnione,
- i nie wszystko z listy powinno być traktowane jako „bugfix”.

Najuczciwszy opis obecnego stanu:

**to nie jest tylko lista błędów; to mieszanka bugfixów, security hardeningu, braków funkcjonalnych i porządków technicznych.**
