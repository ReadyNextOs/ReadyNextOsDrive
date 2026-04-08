# Logowanie Desktop Tokenem

## Cel

Uprościć autoryzację aplikacji desktop ReadyNextOs Drive tak, aby użytkownik nie musiał ręcznie wpisywać:

- `server_url`
- `email`
- `password`

Zamiast tego użytkownik ma wkleić jeden token pobrany z panelu WWW. Desktop wymienia ten token na właściwy token API i zapisuje go lokalnie w systemowym keychainie.

## Założenia

- Token używany do uruchomienia logowania desktopowego nie jest hasłem użytkownika.
- Token nie powinien zawierać surowego hasła użytkownika.
- Token powinien być krótkożyjący.
- Token powinien być jednorazowy albo mieć bardzo ograniczoną liczbę użyć.
- Po stronie desktopa nadal przechowujemy docelowy token API w OS keychain.
- Obecny login `url + email + hasło` może zostać jako fallback dla administratora lub trybu awaryjnego.

## Model rozwiązania

Wprowadzamy osobny typ tokena:

- `desktop_bootstrap_token`

Jego rola:

- przenieść do desktopa minimalny zestaw danych potrzebnych do startu sesji,
- pozwolić desktopowi pobrać właściwy token API,
- nie ujawniać hasła użytkownika.

## Czego nie robić

Nie rekomenduję modelu:

- "token zawiera zaszyfrowane `url + login + hasło`"
- "token jest stale widoczny w oknie pobierania aplikacji"

Powody:

- wyciek takiego tokena jest praktycznie równoważny wyciekowi danych logowania,
- token stale widoczny na stronie łatwo przejąć przez screenshot, shoulder surfing, historię sesji lub XSS,
- rotacja i unieważnianie są trudniejsze,
- model miesza dane uwierzytelniające użytkownika z tokenem transportowym.

## Rekomendowany model

### Wariant preferowany

Token bootstrapowy jest losowym, podpisanym lub zapisanym po stronie serwera sekretem, który reprezentuje tymczasową autoryzację urządzenia.

Token nie musi zawierać jawnie żadnych danych użytkownika. Serwer może trzymać mapowanie:

- `token_id`
- `user_id`
- `tenant_id`
- `server_url`
- `expires_at`
- `used_at`
- `created_by_session_id`

Desktop przekazuje token do endpointu wymiany, a serwer zwraca:

- właściwy token API,
- dane użytkownika,
- dane konfiguracyjne potrzebne desktopowi.

### Wariant dopuszczalny

Token może być samowystarczalny i zawierać podpisane lub zaszyfrowane:

- `server_url`
- `user_id`
- `tenant_id`
- `exp`
- `jti`

Nadal nie powinien zawierać:

- hasła użytkownika,
- długowiecznego tokena API,
- danych, których nie da się szybko unieważnić.

Ten wariant jest gorszy operacyjnie od wariantu preferowanego, bo trudniej go odwołać przed wygaśnięciem, jeśli nie ma centralnej ewidencji `jti`.

## Docelowy UX

### Panel WWW

W widoku pobierania aplikacji Drive użytkownik widzi:

- przycisk `Wygeneruj token do aplikacji desktop`,
- pole z tokenem,
- przycisk `Kopiuj`,
- informację o czasie ważności, np. `ważny przez 10 minut`,
- opcjonalnie przycisk `Unieważnij token`.

Token nie powinien być generowany i wyświetlany stale bez interakcji użytkownika. Lepsze jest generowanie na żądanie.

### Desktop

Ekran logowania ma dwa tryby:

- `Token`
- `Zaawansowane`

Tryb `Token`:

- jedno pole `Token`,
- przycisk `Połącz`,
- opcjonalnie link `Użyj logowania klasycznego`.

Tryb `Zaawansowane`:

- obecne pola `Adres serwera`, `E-mail`, `Hasło`.

## Flow end-to-end

### 1. Użytkownik generuje token w WWW

1. Użytkownik loguje się do aplikacji WWW.
2. Przechodzi do widoku pobierania aplikacji Drive.
3. Kliknie `Wygeneruj token do aplikacji desktop`.
4. Backend tworzy `desktop_bootstrap_token` z TTL, np. 10 minut.
5. Backend zapisuje rekord tokena jako aktywny i nieużyty.
6. WWW pokazuje token użytkownikowi do skopiowania.

### 2. Użytkownik loguje się tokenem w desktopie

1. Użytkownik otwiera desktop.
2. Wybiera tryb `Token`.
3. Wkleja token.
4. Desktop wywołuje endpoint wymiany tokena.
5. Backend weryfikuje token.
6. Jeśli token jest poprawny, aktywny i nieużyty, backend:
   - oznacza go jako użyty,
   - generuje zwykły token API dla desktopa,
   - zwraca dane użytkownika i konfigurację.
7. Desktop zapisuje token API w keychain.
8. Desktop zapisuje konfigurację lokalną i przechodzi do aplikacji.

### 3. Kolejne uruchomienia

1. Desktop nie potrzebuje już bootstrap tokena.
2. Desktop korzysta z tokena API zapisanego w keychain.
3. Przy wylogowaniu usuwany jest token API i lokalna konfiguracja.

## Proponowane endpointy API

### 1. Generowanie tokena w WWW

`POST /api/v1/desktop-auth/tokens`

Request:

```json
{}
```

Response:

```json
{
  "token": "rdt_...",
  "expires_at": "2026-04-08T10:15:00Z",
  "expires_in": 600
}
```

Uwagi:

- endpoint wymaga aktywnej sesji WWW,
- warto dodać rate limiting,
- warto ograniczyć liczbę aktywnych tokenów per user, np. 3.
- odpowiedź może zawierać też `token_id`, jeśli frontend ma wspierać unieważnianie konkretnego tokena.

### 2. Unieważnienie tokena

`DELETE /api/v1/desktop-auth/tokens/{id}`

Lub prostszy wariant:

`POST /api/v1/desktop-auth/tokens/revoke`

### 3. Wymiana tokena w desktopie

`POST /api/v1/desktop-auth/exchange`

Request:

```json
{
  "token": "rdt_...",
  "device_name": "ReadyNextOs Drive"
}
```

Response:

```json
{
  "token": "sanctum_or_jwt_token",
  "user": {
    "id": "usr_123",
    "email": "jan@firma.pl",
    "name": "Jan Kowalski",
    "tenant_id": "ten_123"
  },
  "config": {
    "server_url": "https://docs.firma.pl"
  }
}
```

Błędy:

- `400` invalid token format
- `401` token invalid
- `409` token already used
- `410` token expired

## Specyfikacja backendu PHP/Laravel

### Cel implementacyjny

Backend ma obsłużyć dwa niezależne use-case:

- generowanie krótkowiecznego tokena z poziomu zalogowanej sesji WWW,
- wymianę tego tokena przez aplikację desktop na zwykły token API.

### Proponowana tabela

Migracja `desktop_auth_tokens`:

```php
$table->uuid('id')->primary();
$table->string('token_hash', 255)->unique();
$table->foreignUuid('user_id')->constrained()->cascadeOnDelete();
$table->foreignUuid('tenant_id')->index();
$table->string('server_url', 2048);
$table->timestamp('expires_at')->index();
$table->timestamp('used_at')->nullable()->index();
$table->timestamp('revoked_at')->nullable()->index();
$table->string('created_by_ip', 64)->nullable();
$table->text('created_by_user_agent')->nullable();
$table->timestamps();
```

### Model

Przykładowy model `App\\Models\\DesktopAuthToken`:

- `id`
- `token_hash`
- `user_id`
- `tenant_id`
- `server_url`
- `expires_at`
- `used_at`
- `revoked_at`

Przydatne helpery:

- `isExpired(): bool`
- `isUsed(): bool`
- `isRevoked(): bool`
- `isActive(): bool`
- `markAsUsed(): void`
- `markAsRevoked(): void`

### Serwis generowania tokena

Przykładowy serwis `App\\Services\\DesktopAuthTokenService`:

1. generuje losowy sekret, minimum 32 bajty entropii,
2. buduje token w formacie JWT-like lub opaque token, np. `rdt_xxx`,
3. zapisuje w bazie wyłącznie hash tokena,
4. ustawia `expires_at`, np. `now()->addMinutes(10)`,
5. zwraca plaintext token tylko raz, w odpowiedzi API.

Rekomendacja:

- jeśli ma być prosty bootstrap po stronie desktopu, token może być podpisanym JWT z claimem `server_url`,
- jeśli priorytetem jest pełna kontrola unieważniania, lepszy będzie opaque token z lookupem w bazie.

### Kontroler WWW

`POST /api/v1/desktop-auth/tokens`

Kontroler:

1. pobiera zalogowanego użytkownika z sesji,
2. opcjonalnie czyści stare, nieważne tokeny,
3. sprawdza limit aktywnych tokenów,
4. generuje nowy token,
5. zwraca `token`, `expires_at`, `expires_in`, opcjonalnie `id`.

### Kontroler wymiany tokena

`POST /api/v1/desktop-auth/exchange`

Kontroler:

1. waliduje wejście `token` i `device_name`,
2. odczytuje `server_url` z tokena lub znajduje rekord po hashu,
3. sprawdza, czy token istnieje, nie wygasł, nie został cofnięty i nie był użyty,
4. w transakcji oznacza token jako użyty,
5. generuje normalny token API dla desktopa,
6. zwraca `token`, `user`, `config.server_url`.

Ważne:

- oznaczenie `used_at` musi być atomowe,
- najlepiej użyć transakcji i blokady rekordu `SELECT ... FOR UPDATE`,
- jeśli dwa requesty przyjdą równocześnie, tylko jeden może zakończyć się sukcesem.

### Walidacja Laravel

Przykład requestu dla exchange:

```php
return [
    'token' => ['required', 'string', 'max:4096'],
    'device_name' => ['required', 'string', 'max:255'],
];
```

### Generowanie docelowego tokena API

Jeśli system używa Laravel Sanctum:

```php
$plainTextToken = $user->createToken($deviceName)->plainTextToken;
```

Jeśli potrzeba uprawnień per urządzenie, warto dodać:

- nazwę urządzenia,
- czas utworzenia,
- listę aktywnych tokenów użytkownika do zarządzania sesjami.

### Odpowiedź backendu

```json
{
  "token": "1|sanctum_plain_text_token",
  "user": {
    "id": "usr_123",
    "email": "jan@firma.pl",
    "name": "Jan Kowalski",
    "tenant_id": "ten_123"
  },
  "config": {
    "server_url": "https://docs.firma.pl"
  }
}
```

### Sugestia implementacyjna dla tokena bootstrapowego

Najpraktyczniejszy kompromis dla obecnego desktopu:

- format JWT-like,
- claim `server_url`,
- claim `jti`,
- claim `exp`,
- podpis HMAC lub kluczem aplikacji,
- dodatkowo rekord w bazie z `token_hash` lub `jti`, żeby wspierać jednorazowość i revoke.

Taki model pozwala desktopowi odczytać `server_url` lokalnie przed exchange, ale nadal zachowuje kontrolę po stronie serwera.

## Algorytm po stronie webowej

### Cel UI

W dialogu `Aplikacje desktopowe` użytkownik ma mieć możliwość:

- wygenerowania tokena do desktopu,
- automatycznego skopiowania tokena do schowka po wygenerowaniu,
- ponownego skopiowania tokena ikoną kopiowania,
- zobaczenia czasu ważności tokena,
- wygenerowania nowego tokena bez zamykania dialogu.

### Docelowe zachowanie dialogu

W dolnej części dialogu, pod listą aplikacji desktopowych:

- zamiast samego przycisku `Zamknij`,
- dodać główny przycisk `Generuj token`,
- po wygenerowaniu pokazać pole z tokenem,
- obok pola pokazać ikonę kopiowania,
- po udanym generowaniu automatycznie wykonać kopiowanie do schowka,
- pokazać komunikat `Skopiowano do schowka`,
- zostawić możliwość ręcznego zamknięcia dialogu osobnym przyciskiem lub ikoną zamknięcia.

### Stan komponentu frontendowego

Minimalny stan widoku:

```ts
type DesktopTokenState = {
  token: string | null;
  tokenId: string | null;
  expiresAt: string | null;
  expiresIn: number | null;
  generating: boolean;
  copying: boolean;
  error: string | null;
  copied: boolean;
};
```

### Algorytm frontend krok po kroku

#### 1. Otwarcie dialogu

Po otwarciu dialogu:

- nie generować tokena automatycznie,
- wyczyścić stary stan lokalny,
- ustawić `token = null`,
- ukryć sekcję kopiowania, dopóki token nie zostanie wygenerowany.

#### 2. Kliknięcie `Generuj token`

Po kliknięciu:

1. zablokować przycisk na czas requestu,
2. wykonać `POST /api/v1/desktop-auth/tokens`,
3. odebrać `token`, `expires_at`, `expires_in`, opcjonalnie `id`,
4. zapisać wynik do stanu komponentu,
5. spróbować automatycznie skopiować token do schowka,
6. jeśli kopiowanie się uda:
   - ustawić `copied = true`,
   - pokazać komunikat sukcesu,
7. jeśli kopiowanie się nie uda:
   - zostawić token widoczny,
   - pokazać komunikat `Nie udało się skopiować automatycznie`,
   - pozostawić aktywną ikonę kopiowania.

#### 3. Kliknięcie ikony kopiowania

Po kliknięciu ikony:

1. wywołać `navigator.clipboard.writeText(token)`,
2. po sukcesie ustawić `copied = true`,
3. pokazać krótki komunikat `Skopiowano`,
4. po kilku sekundach zresetować stan komunikatu.

#### 4. Ponowne wygenerowanie tokena

Jeśli użytkownik kliknie `Generuj token` ponownie:

- frontend może od razu pobrać nowy token,
- poprzedni token powinien zostać uznany za zastąpiony,
- backend może opcjonalnie automatycznie cofnąć poprzedni aktywny token tego użytkownika.

### Kontrakt API dla frontendu webowego

Request:

```http
POST /api/v1/desktop-auth/tokens
```

Response:

```json
{
  "id": "dat_123",
  "token": "eyJhbGciOi...",
  "expires_at": "2026-04-08T10:15:00Z",
  "expires_in": 600
}
```

### Funkcje frontendowe

Przykładowy kontrakt TypeScript:

```ts
export interface DesktopAuthTokenResponse {
  id: string;
  token: string;
  expires_at: string;
  expires_in: number;
}

export async function generateDesktopAuthToken(): Promise<DesktopAuthTokenResponse> {
  const response = await api.post('/api/v1/desktop-auth/tokens');
  return response.data;
}

export async function copyToClipboard(value: string): Promise<void> {
  await navigator.clipboard.writeText(value);
}
```

### Reguły UX

- przycisk `Generuj token` ma być głównym CTA,
- token powinien być pokazany w polu readonly,
- pole tokena powinno umożliwiać ręczne zaznaczenie,
- ikona kopiowania powinna być aktywna tylko gdy token istnieje,
- po automatycznym skopiowaniu warto pokazać status sukcesu przy polu,
- jeśli przeglądarka blokuje clipboard API, UI nie może blokować użytkownika.

### Obsługa błędów

Frontend powinien rozróżnić:

- błąd generowania tokena,
- błąd kopiowania do schowka,
- wygaśnięcie aktualnie pokazanego tokena.

Przykładowe komunikaty:

- `Nie udało się wygenerować tokena. Spróbuj ponownie.`
- `Token wygenerowany, ale nie udało się skopiować go automatycznie.`
- `Token wygasł. Wygeneruj nowy.`

### Timer ważności

Jeśli frontend pokazuje licznik czasu:

1. po otrzymaniu `expires_at` uruchomić lokalny countdown,
2. po dojściu do zera oznaczyć token jako wygasły,
3. zablokować ikonę kopiowania albo zostawić ją aktywną tylko informacyjnie,
4. wyróżnić przycisk `Generuj token` jako następną akcję.

### Pseudokod komponentu

```ts
async function onGenerateClick() {
  setState({ generating: true, error: null, copied: false });

  try {
    const result = await generateDesktopAuthToken();
    setState({
      token: result.token,
      tokenId: result.id,
      expiresAt: result.expires_at,
      expiresIn: result.expires_in,
      generating: false,
      copied: false,
      error: null,
    });

    try {
      await copyToClipboard(result.token);
      setState((prev) => ({ ...prev, copied: true }));
    } catch {
      setState((prev) => ({
        ...prev,
        error: 'Token wygenerowany, ale nie udało się skopiować go automatycznie.',
      }));
    }
  } catch {
    setState((prev) => ({
      ...prev,
      generating: false,
      error: 'Nie udało się wygenerować tokena. Spróbuj ponownie.',
    }));
  }
}
```

### Decyzja implementacyjna dla weba

Najprostszy wariant do wdrożenia:

- przycisk `Generuj token` w dialogu,
- po wygenerowaniu readonly input z tokenem,
- automatyczne `copy to clipboard`,
- obok inputa ikona kopiowania do ponownego użycia,
- informacja `Ważny przez 10 minut`.

To wystarczy, żeby zespół webowy mógł przygotować komponent i algorytm bez dalszych założeń.

## Struktura danych po stronie backendu

Przykładowa tabela `desktop_auth_tokens`:

- `id`
- `token_hash`
- `user_id`
- `tenant_id`
- `server_url`
- `created_at`
- `expires_at`
- `used_at`
- `revoked_at`
- `created_by_ip`
- `created_by_user_agent`

Uwagi:

- w bazie trzymamy hash tokena, nie token w plaintext,
- sam token powinien być generowany z odpowiednią entropią,
- porównanie po stronie serwera powinno być stałoczasowe.

## Bezpieczeństwo

### Wymagania minimalne

- tylko HTTPS,
- token bootstrapowy ważny krótko, np. 10 minut,
- token jednorazowy,
- możliwość unieważnienia,
- hash tokena w bazie,
- audyt: kto wygenerował, kiedy użyto, z jakiego IP.

### Rekomendacje

- nie pokazywać tokena automatycznie po wejściu na stronę,
- pokazywać token dopiero po kliknięciu `Wygeneruj`,
- domyślnie ukrywać token za przyciskiem `Pokaż`,
- pozwolić skopiować token jednym kliknięciem,
- po użyciu od razu oznaczać token jako zużyty,
- ograniczyć tempo generowania i wymiany tokenów,
- logować zdarzenia bezpieczeństwa.

### Opcjonalne wzmocnienia

- powiązanie tokena z aktywną sesją WWW,
- wymuszenie potwierdzenia hasłem lub 2FA przed wygenerowaniem,
- wyświetlenie listy aktywnych desktopowych sesji urządzeń,
- możliwość zdalnego wylogowania urządzenia.

## Zmiany potrzebne w desktopie

### Frontend

W [src/pages/LoginPage.tsx](/home/mariusz/GitApps/ReadyNextOsDrive/src/pages/LoginPage.tsx):

- dodać przełącznik trybu logowania,
- dodać pole `token`,
- dodać osobny submit dla logowania tokenem,
- zachować obecne logowanie klasyczne jako fallback.

W [src/lib/tauri.ts](/home/mariusz/GitApps/ReadyNextOsDrive/src/lib/tauri.ts):

- dodać funkcję `loginWithToken(token: string)`.

### Tauri / Rust

W [src-tauri/src/main.rs](/home/mariusz/GitApps/ReadyNextOsDrive/src-tauri/src/main.rs):

- dodać nową komendę Tauri, np. `login_with_token`.

W [src-tauri/src/auth.rs](/home/mariusz/GitApps/ReadyNextOsDrive/src-tauri/src/auth.rs):

- dodać funkcję `exchange_desktop_token`,
- zwracać ten sam typ odpowiedzi użytkownika, co klasyczne logowanie,
- ustawić timeout dla `reqwest::Client`.

### Konfiguracja lokalna

Po wymianie tokena desktop powinien zapisać:

- `server_url`,
- `user_email`,
- `tenant_id`,
- token API w keychain.

Bootstrap token nie powinien być nigdzie trwale przechowywany po udanym logowaniu.

## Proponowany kontrakt odpowiedzi po stronie desktopu

Żeby uprościć integrację, oba tryby logowania powinny kończyć się tym samym obiektem odpowiedzi:

```json
{
  "token": "api_token",
  "user": {
    "id": "usr_123",
    "email": "jan@firma.pl",
    "name": "Jan Kowalski",
    "tenant_id": "ten_123"
  },
  "server_url": "https://docs.firma.pl"
}
```

Dzięki temu logika zapisu do configu i keychaina może być wspólna.

## Plan wdrożenia

### Etap 1

- dodać backendowy model `desktop_auth_tokens`,
- dodać endpoint generowania tokena,
- dodać endpoint wymiany tokena,
- dodać audyt i unieważnianie.

### Etap 2

- dodać drugi tryb logowania w desktopie,
- wdrożyć komendę Tauri `login_with_token`,
- zapisywać wynik tak samo jak przy zwykłym loginie.

### Etap 3

- dopracować UX w panelu WWW,
- dodać informację o ważności tokena,
- dodać listę aktywnych urządzeń lub sesji.

## Decyzje projektowe do potwierdzenia

- Czy token ma być jednorazowy czy np. ważny do 3 użyć.
- Jaki ma być TTL, rekomendacja: 10 minut.
- Czy generowanie tokena wymaga ponownego potwierdzenia hasła lub 2FA.
- Czy klasyczne logowanie zostaje dostępne dla wszystkich, czy tylko jako fallback.
- Czy po stronie WWW pokazujemy jeden aktywny token, czy listę ostatnich tokenów.

## Rekomendacja końcowa

Najbezpieczniejszy i najprostszy operacyjnie model:

- token bootstrapowy generowany na żądanie,
- ważny krótko,
- jednorazowy,
- bez hasła użytkownika,
- wymieniany na normalny token API,
- token API przechowywany lokalnie w OS keychain.

To daje prosty UX podobny do logowania QR w mobile, ale bez przenoszenia właściwego hasła do aplikacji desktop.
