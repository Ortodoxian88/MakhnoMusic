# MakhnoYMMod 2.2.3

Форк-мод десктопного клиента Яндекс Музыки. Rust-патчер, чистая телеметрия,
кроссплатформенность (Windows/Linux), без мёртвых зависимостей.

> Название мода везде: **MakhnoYMMod**, версия **2.2.3**.

## Что нового в 2.2.3

Полный проход по всем находкам ревизии (включая мелочи):

**Патчер (Rust):**
- починен весь `.exe`-флоу (раньше не работал вовсе): 7-Zip теперь корректно
  вызывается со слитным `-o<path>`, проверка наличия 7z идёт командой `i`
  (7-Zip не знает `--version` и отвечал кодом 7), внутренний `app-64.7z`
  ищется рекурсивно (у NSIS-инсталлеров он лежит в `$PLUGINSDIR`);
- повторный патч своего deb больше не удваивает суффикс версии
  (`+makhno2.2.2+makhno2.2.3` → чистый `+makhno2.2.3`);
- `md5sums` в пересобранном deb регенерируется по факту (раньше `dpkg -V`
  рапортовал «повреждённые» файлы);
- бэкап `app.asar.orig` (57 МБ) больше не пакуется внутрь deb;
- HTML-инъекция применяется только к страницам приложения (`app/`), доки
  node_modules не трогаются;
- защита от битых asar (границы блоба, проверки заголовка) вместо паник;
- CLI: опечатки во флагах больше не проходят молча, поддержан `--флаг=значение`;
- вычищены все предупреждения компилятора и мёртвый код.

**Мод (JS):**
- скачивание треков: расшифрованный файл больше не перекодируется ffmpeg
  «по умолчанию» (была потеря качества: mp3 320k пересохранялся в 128k) —
  теперь один remux `-c copy` с тегами и обложкой, а при сбое ffmpeg в папке
  не остаётся битого трека (раньше оставался полусобранный файл);
- `windowsHide` для ffmpeg/where на Windows — консольные окна не мигают;
- `crypto.subtle` берётся из `node:crypto` (фолбэк для старых Electron);
- первый запуск: настройки пишутся после mkdir, без риска краша;
- Discord RPC: `clearActivity` только при реальном переходе в паузу (не
  каждые 500 мс), старый клиент корректно уничтожается при реконнекте;
- axios-мост: проверка config.url, редиректы только в доверенные зоны
  (белый список нельзя обойти перенаправлением).

**Дистрибутивы:**
- добавлен полноценный Windows-инсталлер `MakhnoYMMod-Setup-2.2.3-win-x64.exe`
  (NSIS, per-user, без прав администратора): ставит клиент 5.128.0 с уже
  встроенным модом, ярлыки, деинсталлятор, запись в «Программах и компонентах»;
- клиент обновлён до 5.128.0 (Linux deb и Windows asar).

## Что делает мод

- скачивание треков (ffmpeg, безопасный `execFile` с массивом аргументов,
  remux без перекодирования);
- Discord RPC (богатый статус плеера);
- настройки мода внутри клиента (`mod_settings.json`);
- тёмно-зелёная тема вместо жёлтой: акцент `#2E7D32` по умолчанию, перекрашены все
  CSS-переменные клиента, favicon, SVG-иконки и inline-SVG в JS-чанках
  (жёлтый текст на кнопках заменён белым для читаемости);
- блокировщик телеметрии Яндекса (17 правил `webRequest`);
- хоткей `Ctrl+Shift+I` (DevTools), `webSecurity` отключён для работы мода;
- системный тулбар по настройке `devtools/systemToolbar`;
- минимальный размер окна 360×550, окно показывается сразу;
- автообновление клиента отключено (обновляется только мод через meta.json).

## Что вырезано из старой сборки (YandexMusicMod 2.2.0)

| Вырезано | Причина |
| --- | --- |
| Sentry DSN `o4510880724353024.ingest.de.sentry.io` | собственная телеметрия мода на немецкий инстанс — удалена, `dsn: ""` |
| ссылки на Discord-сообщество оригинального автора (`discord.gg/4nK7nk2sY8`) | кнопка и баннер удалены; Discord RPC работает |
| `fluent-ffmpeg` | мёртвая зависимость, не использовалась |
| `ts-md5` | замена встроенному `crypto.createHash` |
| `ffmpeg-static` (79 МБ) | системный ffmpeg или `--bundle-ffmpeg` по желанию |
| `zod`, `neverthrow`, `cmdk`, `@sentry/react`, `@radix-ui/react-slider` | не использовались в main-процессе |
| сплэш-видео `app/media/splash_screen` | быстрее запуск |
| `child_process.exec` со склейкой строк | заменён на `execFile` с массивом аргументов |
| `start ""` (Windows-only) | заменён на `electron.shell.openPath` |

## Установка

### Windows — инсталлером (проще всего)

1. Запустите `MakhnoYMMod-Setup-2.2.3-win-x64.exe`.
2. Дальше «Далее → Установить → Готово»: инсталлер ставит Яндекс Музыку 5.128.0
   с уже встроенным модом в `%LOCALAPPDATA%\Programs\MakhnoYMMod`
   (права администратора не нужны), создаёт ярлыки и «Удаление программ».
3. Для скачивания треков установите ffmpeg: `winget install ffmpeg`.
4. Запустите. В меню мода версия должна показывать `v2.2.3`.

Деинсталляция — стандартно через «Параметры → Приложения» или
`Uninstall.exe` в каталоге установки.

### Windows — вручную (app.asar)

1. Закройте Яндекс Музыку.
2. В каталоге установки откройте `resources\`.
3. Сохраните старый `app.asar`.
4. Замените `app.asar` файлом `MakhnoYMMod-2.2.3-windows-5.128.0-app.asar`.
5. Удалите каталог `app.asar.unpacked` (79 МБ ffmpeg больше не нужен).
6. Для скачивания треков установите ffmpeg (`winget install ffmpeg`).

### Linux (Ubuntu 22.04+/Debian 11+)

```bash
sudo apt install ./makhnoymod_5.128.0-r22145392+makhno2.2.3_amd64.deb
sudo apt install ffmpeg        # для скачивания треков
```

Пакет называется `makhnoymod`, конфликтует/заменяет `yandex-music` и `yandexmusic`.

## Сборка патчера

```bash
cargo build --release
# бинарник: target/release/makhno-patcher
```

Зависимости крейта: `serde_json` (preserve_order) и `regex` — только чистый Rust,
никаких системных библиотек. Windows-бинарник собирается кросс-компиляцией:

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

## Использование

```text
makhno-patcher patch --app-dir <цель> [--payload-dir DIR] [опции]

цель:
  каталог клиента Windows   (содержит resources\app.asar)
  каталог клиента Linux     (/opt/Яндекс Музыка или распакованный deb)
  .asar-файл                (патчит на месте, создаёт app.asar.orig)
  .deb-файл                 (распаковывает, патчит, пересобирает deb)
  корень deb                (root/ + DEBIAN/, пересобирает makhnoymod_*.deb)
  .exe-инсталлятор          (нужен 7z; достаёт и патчит app.asar)

опции:
  --repo-url URL        кнопки «GitHub» в UI мода (по умолчанию этот репозиторий)
  --meta-url URL        meta.json для проверки обновлений мода
  --keep-web-security   не отключать webSecurity (лирика musixmatch может не работать)
  --bundle-ffmpeg PATH  вшить ffmpeg в app.asar.unpacked (node_modules/ffmpeg-static)
  --no-deb              не пересобирать deb
  --dry-run             только отчёт

makhno-patcher verify --target <asar|каталог|.deb>   # 81 проверка пропатченной сборки
makhno-patcher unpack --asar A --out DIR        # отладка
makhno-patcher pack --dir DIR --out A           # отладка
```

Флаги значения можно передавать и в форме `--флаг=значение`.

## Сборка Windows-инсталлера

```bash
# 1. распаковать официальный инсталлер и патчит asar
makhno-patcher patch --app-dir Yandex_Music_x64_5.128.0.exe
# 2. положить распакованное дерево в installer/app и собрать NSIS
makensis -INPUTCHARSET UTF8 installer/makhno-setup.nsi
```

## Структура репозитория

```
├── src/                 исходники патчера (Rust)
│   ├── main.rs          CLI и диспетчер целей
│   ├── asar.rs          чтение/запись Electron-asar
│   ├── patch.rs         патчи index.js/preload.js/package.json/HTML
│   ├── payload.rs       загрузка payload и подстановка токенов
│   ├── verify.rs        81 проверка результата
│   ├── deb.rs           пересборка deb (ar/tar/xz, md5sums), распаковка exe (7z)
│   └── cli.rs           парсер аргументов
├── payload/             содержимое мода
│   ├── head.js          пролог index.js (настройки до старта приложения)
│   ├── main.js          main-процесс мода (IPC, загрузки, Discord RPC)
│   ├── preload.js       contextBridge-мост window.makhnoYMMod
│   ├── app/makhnoYMMod/ renderer.js (SPA), renderer.css, шрифты
│   └── node_modules/    axios, sanitize-filename, @xhayper/discord-rpc + дерево
├── .meta/meta.json      {"modStable":"2.2.3"} — проверка обновлений в UI
├── .github/workflows/   CI: сборка linux/windows бинарников
├── installer/           NSIS-скрипт Windows-инсталлера (makhno-setup.nsi)
└── scripts/             independent_verify.py — независимая python-верификация
```

## Верификация

Встроенная: `makhno-patcher verify --target app.asar` — 81 проверка
(маркеры патчей, 8 IPC-каналов в трёх файлах, отсутствие DSN и legacy-имён,
зависимости, HTML-инъекция, `node --check` при наличии node).

Независимая: `python3 scripts/independent_verify.py app.asar` — python-реализация
тех же проверок без Rust-кода.

## Патчи index.js (справка)

| Что | Как |
| --- | --- |
| пролог | `// MakhnoYMMod head begin … end` перед кодом клиента |
| апдейтер | `constructor() { return;` + `updater.start()` → комментарий |
| окно | `minWidth 768→360`, `minHeight 650→550`, `show: true`, тулбар по настройке |
| webPreferences | ` devTools: true, ` + `webSecurity: false` (или `--keep-web-security`) |
| телеметрия | `webRequest.onBeforeRequest` — 17 URL-правил `cancel: true` |
| main-блок | `// MakhnoYMMod main begin … end` — IIFE в конце файла |
| перекраска | жёлтое семейство (#fed42a/#ff0/#ffd466/…) в CSS, SVG и чанках -> #2E7D32 |

Повторный патч (repatch) своего же выхода или старого YandexMusicMod идемпотентен.

## Лицензия и авторы

- Патчер и обвязка — MIT, Makhno.
- Исходный мод YandexMusicMod (Stephanzion), компоненты их авторов
  (renderer содержит MIT-библиотеки React-экосистемы).
- Десктоп-клиент «Яндекс Музыка» и все права на него — © Яндекс.
