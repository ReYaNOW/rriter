# Markdown-ридер: картинки, Mermaid, ссылки, оглавление — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read-режим markdown показывает картинки (локальные и http, png/jpeg/gif/webp/svg) и Mermaid-диаграммы, ссылки кликабельны, есть попап оглавления в Read и Edit.

**Architecture:** Общий на приложение медиа-кэш `MarkdownMedia` (App-сторона) читает файлы и http в фоне через `UiWaker::spawn_one_shot`, а всё декодирование (растр, SVG, Mermaid) отдаёт дочернему процессу того же exe (`--rriter-media-render`), потому что в release `panic = "abort"`. Раскладка Read-режима читает натуральные размеры из кэша, рисует заглушку до готовности и пересчитывается по `media_gen` с привязкой прокрутки. Навигация (цели ссылок, slug'и, оглавление) — модуль `markdown_nav` поверх `MarkdownDocument`.

**Tech Stack:** Rust, tree-sitter-md, glow/OpenGL, `image` 0.25, `resvg`/`usvg` 0.47, `mermaid-rs-renderer`, `reqwest` blocking.

**Spec:** `docs/superpowers/specs/2026-10-01-markdown-reader-media-design.md` — исполнитель читает разделы, указанные в задаче, целиком.

**Ветка:** `markdown-media` от свежего `master`. **Workspace SDD:** `.superpowers/sdd/2026-10-01-markdown-reader-media/` (ledger `progress.md`).

## Global Constraints

- Никакого `rustfmt`/`cargo fmt`; стиль соседних строк — руками. Исходники не править через `sed -i` и перенаправления (логи в новые файлы можно).
- Нет `.unwrap()`/`.expect()` в продакшн-путях (в тестах можно).
- В кадре (draw/render/prepare) нет дискового и сетевого I/O, декодирования, растеризации, запуска процессов. Запуск фоновых потоков (`spawn_one_shot` из `request`/`poll`/`prepare_gpu`) разрешён: потоки не блокируют кадр, I/O и процессы живут внутри них.
- Не больше двух загрузок текстур за кадр; CPU-пиксели освобождаются сразу после загрузки на GPU.
- Лимиты: http таймаут 15 с, ответ и локальный файл ≤ 20 МБ; в хелпере `image::Limits` `max_alloc` 128 МБ и длинная сторона входа ≤ 16384 px; растр ≤ 4096 px по длинной стороне; ≤ 3 слотов задач (= ≤ 3 хелпер-процессов); бюджет текстур 128 МБ сверх видимых; дисковый кэш ≤ 200 МБ (`platform::cache_dir()/markdown-images/`); хелпер — таймаут 10 с.
- Внешние ресурсы SVG (`<image href>`, внешние стили) не загружаются никогда. Шрифты хелпера — только встроенные шрифты редактора (`src/fonts/Inter-Regular.otf`, `src/fonts/JetBrainsMonoNerdFont-Regular.ttf`, как в `src/renderer/renderer_init_methods.rs:469-470`), без сканирования системных.
- `panic = "abort"` в `Cargo.toml` не менять.
- Новые файлы ≥ 200 строк; ни один файл > 1600 строк (жёсткое правило AGENTS.md); `src/render_view/markdown_read.rs` (1487) растёт не больше чем на +50 строк в Task 6 и +40 в Task 8; превышение бюджета — вынести единицу поведения в отдельный файл в той же задаче. `src/app/app_state.rs` (1495) — по одному полю на задачу, не больше.
- Каждая задача сама вносит свои новые файлы в `PROJECT_GUIDE.md` §4 (отдельного шага в конце нет).
- Логика фичи — в собственных типах (`MarkdownMedia`, модуль `markdown_nav`); `impl App` только маршрутизирует. Никаких новых `static`/`OnceLock` с данными приложения.
- Процессы — только через `platform::ManagedChild` / `platform::run_command_output` (`src/platform/process.rs`); http — только `platform::blocking_http_client_builder`; открытие URL — `platform::open_url`.
- UI-координаты округляются (`(N * s).round()`), текст — от целых базовых линий.
- Тесты: `make test TEST_FILTER=<module path>` в foreground, вывод в `/tmp/rriter-md-t<N>.log`, смотреть `tail`/`grep`; ≤ 3 прогонов на шаг «до зелёного», потом отчёт с падениями. Release-сборку implementer не запускает (её делает `make codex_test` у контроллера). UI-поведение для headless-тестов — сначала `dump`/`shot` на предсобранном release-бинарнике (`docs/headless.md`; он собран до ветки и новой фичи не содержит — пробы только на существующее поведение). Асинхронное в headless-тестах ждать `tests_support::wait_until`, не фиксированным `wait`. Всё, что тест пишет вне процесса (каталоги, порты), уникально на процесс (`std::process::id()`, порт 0). Каждый новый headless-файл регистрируется в `src/headless/mod.rs` рядом со строкой 77.
- Внешнее нельзя: push, PR, комментарии, ssh. Коммитит контроллер SDD, не implementer.
- Warning clippy в тронутом коде чинится по-настоящему, без `#[allow]` (`scripts/lint_changed.py` в `make codex_test`).

## Review Focus

1. Картинка догрузилась, пока идёт анимация колеса, переход к якорю или переход Read/Edit — текст под курсором не прыгает, анимация доезжает до того же места текста (Task 7).
2. Документ с десятками крупных картинок и быстрый скролл туда-обратно — нет зацикленного перерендера, память в бюджете, UI не подвисает, одновременно ≤ 3 хелпер-процессов (Task 4, тест вытеснения; Task 10, фикстура с 30 картинками).
3. Перетаскивание мышью, начатое на ссылке, выделяет текст и ничего не открывает; клик без движения открывает (Task 8).
4. `Ctrl+Shift+O` в не-markdown вкладке и `Ctrl+O` везде работают как раньше (Task 9).
5. Файл картинки изменили или удалили на диске при открытом документе — картинка обновляется / показывается рамка «не найдено», без перезапуска (Task 4 `revalidate_files` и инвалидация, Task 7 проводка триггеров ревалидации).

## Общие интерфейсы

| Имя | Где | Кто создаёт → кто использует |
|---|---|---|
| `MediaKey { File(PathKey), Url(String), Mermaid(u64) }` (`Clone, Hash, Eq, Debug`) | `src/markdown_media.rs` | T1 → T3, T4, T5, T6, T7 |
| `MediaSource { File(PathBuf), Url(String), Mermaid(String) }` | `src/markdown_media.rs` | T1 → T3, T4, T5 |
| `MediaKind { Raster, Svg, Mermaid }` | `src/markdown_media.rs` | T1 → T2, T3 |
| `MediaRequest { key: MediaKey, source: MediaSource, max_raster_w: u32, scale: f32 }` | `src/markdown_media.rs` | T1 → T3, T4, T7 |
| `struct FileStamp { mtime: SystemTime, len: u64 }`; `MediaPixels { natural_w: f32, natural_h: f32, raster_w: u32, raster_h: u32, rgba: Vec<u8>, stamp: Option<FileStamp> }` (RGBA8, straight alpha; `stamp` заполнен для `File`) | `src/markdown_media.rs` | T1 → T2, T3, T4 |
| `MediaError { NotFound, Http(u16), Timeout, TooLarge, TooManyPixels, Decode, Unsupported, Mermaid(String), Crashed }` + `fn label(&self) -> Cow<'static, str>` (рус.: `не найдено`, `HTTP 404`, `таймаут`, `больше 20 МБ`, `слишком большая`, `не удалось декодировать`, `не поддерживается`, текст ошибки Mermaid, `рендер упал`); код в dump — `Debug`-имя варианта без полей | `src/markdown_media.rs` | T1 → T2, T3, T4, T6, T7 |
| `enum RenderCommand { Process { exe: PathBuf, prefix_args: Vec<String> }, InProcess }` | `src/markdown_media.rs` | T1 → T3, T4 |
| `FetchEnv { cache_dir: PathBuf, http: reqwest::blocking::Client, max_bytes: u64, render: Option<RenderCommand> }`; `fn fetch_bytes(source: &MediaSource, env: &FetchEnv) -> Result<(MediaKind, Vec<u8>, Option<FileStamp>), MediaError>`; `fn trim_disk_cache(dir: &Path, limit_bytes: u64)` | `src/markdown_media/fetch.rs` | T1 → T3, T4 |
| `fn decode_in_process(kind: MediaKind, bytes: &[u8], scale: f32, max_raster_w: u32) -> Result<MediaPixels, MediaError>` (растр, SVG, Mermaid; размер растра, шрифты, тема mmdr) | `src/markdown_media/decode.rs` | T2 → T3 |
| `RenderCommand::for_current_process() -> Option<RenderCommand>`; `fn render_media(cmd: &RenderCommand, kind: MediaKind, bytes: &[u8], scale: f32, max_raster_w: u32, timeout: Duration) -> Result<MediaPixels, MediaError>`; `fn run_media_helper_if_requested() -> Option<i32>` | `src/markdown_media/render_helper.rs` | T3 → T4, `main.rs` |
| `fn load_media(req: &MediaRequest, env: &FetchEnv) -> Result<MediaPixels, MediaError>` (= `fetch_bytes` + `render_media`; `env.render == None` → `Unsupported`) | `src/markdown_media/fetch.rs` | T3 → T4 |
| `MarkdownMedia` + `new(env: FetchEnv)`, `with_loader(loader: Arc<dyn Fn(&MediaRequest) -> Result<MediaPixels, MediaError> + Send + Sync>)`, `request(&mut self, req: MediaRequest, waker: &UiWaker)`, `poll(&mut self, waker: &UiWaker) -> bool`, `media_gen(&self) -> u64`, `entry(&self, key: &MediaKey) -> MediaEntryView<'_>`, `prepare_gpu(&mut self, renderer: &mut Renderer, visible: &[VisibleMedia], waker: &UiWaker) -> bool` (true — нужен ещё кадр), `invalidate_path(&mut self, path: &PathKey)`, `revalidate_files(&mut self, keys: &[MediaKey], waker: &UiWaker)`, `reset_failed(&mut self, keys: &[MediaKey])`, `stats(&self) -> MediaStats`; `struct MediaStats { loads_started: u64, texture_bytes: u64, visible_texture_bytes: u64 }`. `release_all` нет: текстуры удалённых записей уходят в список к освобождению в начале `prepare_gpu` | `src/markdown_media.rs` | T4 → T6, T7, T10 |
| `enum MediaEntryView<'a> { Unknown, Pending { natural: Option<(f32, f32)> }, Ready { natural_w: f32, natural_h: f32, texture: Option<&'a glow::Texture> }, Failed(&'a MediaError) }`; `struct VisibleMedia { key: MediaKey, display_w: u32, display_h: u32 }` | `src/markdown_media.rs` | T4 → T6, T7 |
| `struct MarkdownHeading { level: u8, text: String, source_range: Range<usize> }`; `MarkdownDocument::headings(&self, source: &str) -> Vec<MarkdownHeading>`; `MarkdownDocument::link_definitions(&self, source: &str) -> Vec<(String, String)>` (нормализованная метка, destination) — оба метода новые | `src/languages/markdown.rs` | T5 → T7, T8, T9 |
| `fn heading_slugs(headings: &[MarkdownHeading]) -> Vec<String>`; `enum LinkTarget { External(String), Anchor(String), File { path: PathBuf, anchor: Option<String> }, Unsupported }`; `fn resolve_link(dest: &str, doc_dir: &Path, defs: &[(String, String)]) -> LinkTarget`; `struct MediaItem { key: MediaKey, source: MediaSource, alt: String, link: Option<LinkTarget>, source_range: Range<usize> }`; `fn media_paragraph(doc: &MarkdownDocument, block_index: usize, source: &str, doc_dir: &Path, defs: &[(String, String)]) -> Option<Vec<MediaItem>>`; `fn mermaid_item(code: &str, source_range: Range<usize>) -> MediaItem` | `src/app/markdown_nav.rs` | T5 → T7, T8, T9 |
| Чистая функция раскладки над `(natural, state)` элементов и шириной колонки; `ReadBlockKind::Media { items: Vec<PlacedMedia> }`, `struct PlacedMedia { key: MediaKey, x: f32, y: f32, w: f32, h: f32, alt: String }` (коорд. относительно блока, целые; поле `link` добавляет T8); `MarkdownReadLayoutCache::media_blocks()` (`pub(crate)`, для dump); `media_gen`, с которым построен кэш раскладки | `src/render_view/markdown_read_media.rs`, `markdown_read.rs` | T6 → T7, T8 |
| `MarkdownReadLayoutCache::links(&self) -> &[LinkTarget]`, `link_at(&self, x: f32, y: f32) -> Option<u32>`; `StyledRun.link: Option<u32>`; `PlacedMedia.link: Option<u32>` | `markdown_read.rs`, `markdown_read_interaction.rs`, `markdown_read_media.rs` | T8 → T9 не использует |
| `enum LinkAction { OpenUrl(String), ScrollTo(Range<usize>), OpenMarkdown { path: PathBuf, anchor: Option<String> }, OpenFile(PathBuf), None }`, `fn link_action(target: &LinkTarget, headings: &[MarkdownHeading], slugs: &[String]) -> LinkAction` | `src/app/markdown_nav.rs` | T8 → T9 (переход к заголовку) |
| `MarkdownTabState.pending_anchor: Option<String>` | `src/app/markdown.rs` | T8 |
| dump: `tabs[i].markdown_media: [{key, state, x, y, w, h}]`, верхний уровень `markdown_media_stats` (из `stats()`), `markdown_toc: {open, items, selected}` | `src/headless/dump.rs` | T7 (первые два), T9 (третий) → T10 |

---

### Task 1: Типы медиа и получение байтов (файл, http, дисковый кэш)

**Files:**
- Create: `src/markdown_media.rs` (типы T1 из таблицы, включая `RenderCommand`, `FileStamp`, `MediaError::label`; `mod fetch;` — остальные подмодули добавляют следующие задачи; заглушек-файлов нет), `src/markdown_media/fetch.rs`
- Modify: `src/main.rs` (`mod markdown_media;`), `PROJECT_GUIDE.md` §4
- Test: `#[cfg(test)] mod tests` в `fetch.rs`

**Interfaces:** Produces — `MediaKey`, `MediaSource`, `MediaKind`, `MediaRequest`, `FileStamp`, `MediaPixels`, `MediaError`, `RenderCommand`, `FetchEnv`, `fetch_bytes`, `trim_disk_cache`.

Решения (спека §4.2, §4.4):
- `FetchEnv.http` — клиент, который один раз строит `MarkdownMedia` (Task 4) из `platform::blocking_http_client_builder` с таймаутом 15 с. Тесты строят свой клиент с `.no_proxy()`: платформенный билдер добавляет системный прокси (`src/platform/integration.rs:732-745`), и loopback-сервер теста мог бы уйти через него.
- `fetch_bytes(File(path))`: `metadata` → нет файла `NotFound`; размер > `max_bytes` → `TooLarge` без чтения; иначе `std::fs::read`. Возвращает `FileStamp { mtime, len }` из той же `metadata`. Вид: `.svg` → `Svg`, иначе по сигнатуре тела (`<svg` или `<?xml` после пробелов/BOM → `Svg`), иначе `Raster`.
- `fetch_bytes(Url(u))`: схема не `http`/`https` → `Unsupported`. Имя файла кэша — `<cache_dir>/<016x>`, где хеш — FNV-1a 64 от URL, реализованный тут же (несколько строк): он стабилен между версиями компилятора, `DefaultHasher` — нет. Вид определяется по сигнатуре тела и `Content-Type`, отдельного маркера не нужно. Есть в кэше → читаем. Иначе запрос через `env.http`; статус не 2xx → `Http(code)`; `Content-Length` > `max_bytes` → `TooLarge` без чтения тела; тело через `Read::take(max_bytes + 1)`, превышение → `TooLarge`; таймаут и сетевые ошибки без статуса → `Timeout`. Успех пишется в кэш атомарно (временный файл в том же каталоге + `rename`); ошибка записи кэша не ломает результат. Вид: `Content-Type: image/svg+xml` или сигнатура → `Svg`, иначе `Raster`.
- `fetch_bytes(Mermaid(code))` → `(Mermaid, code.into_bytes(), None)`.
- `trim_disk_cache(dir, limit)`: удаляет самые старые по mtime файлы, пока сумма > лимита; ошибки игнорируются. Вызывает `MarkdownMedia` (Task 4).

- [ ] **Step 1:** Тесты (сначала красные). Позитивные: файл PNG-байтов → `(Raster, bytes, Some(stamp))`, `stamp.len` = размер файла; файл `.svg` и файл без расширения с `<svg` → `Svg`; http 200 → байты, запись в кэш; повторный запрос после остановки сервера отдаётся из кэша; имя файла кэша — 16 hex и одинаково для одного URL; `Content-Type: image/svg+xml` → `Svg`; `trim_disk_cache` оставляет новейшие файлы в лимите; `MediaError::label` для каждого варианта. Негативные: нет файла → `NotFound`; файл больше `max_bytes` (в тесте 1 КБ) → `TooLarge`; http 404 → `Http(404)`; `Content-Length` > лимита → `TooLarge`; тело без `Content-Length` длиннее лимита → `TooLarge`; сервер молчит дольше таймаута клиента (в тесте 300 мс) → `Timeout`; `data:image/png;base64,AAAA` и `file:///etc/passwd` → `Unsupported`. HTTP-сервер — `std::net::TcpListener` на `127.0.0.1:0` в потоке теста; клиент теста с `.no_proxy()`; `cache_dir` — уникальный на процесс (`std::process::id()`).
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_media`.

**Агенты:** implementer ds-low-agentic · reviewer нет

### Task 2: Декодирование в процессе: зависимости, растр, SVG, Mermaid

**Files:**
- Modify: `Cargo.toml` (`image` фичи `jpeg`, `gif`, `webp` к `png`; `usvg` — фича текста, если её нет; `mermaid-rs-renderer` с `default-features = false` и минимумом фич для SVG-строки), `src/markdown_media.rs` (`mod decode;`), `PROJECT_GUIDE.md` §4
- Create: `src/markdown_media/decode.rs` (`decode_in_process`, функция размера растра, база шрифтов, тема Mermaid)
- Test: `#[cfg(test)] mod tests` в `decode.rs`

**Interfaces:** Consumes — типы T1. Produces — `decode_in_process`.

Решения (спека §2.3, §2.4, §4.2):
- `Raster`: `image::ImageReader::with_guessed_format` + `Limits` (`max_alloc` 128 МБ, `max_image_width/height` 16384) → превышение `TooManyPixels`, прочие ошибки `Decode`; GIF — первый кадр.
- `Svg`: `usvg::Options` с `resources_dir = None` и резолвером `<image href>`, возвращающим `None` для любых href; `fontdb` наполнить только двумя встроенными шрифтами редактора (`include_bytes!` тех же файлов, что в `renderer_init_methods.rs:469-470`); `resvg` в `tiny_skia::Pixmap`; premultiplied → straight alpha (проверить, что ждёт `Renderer::upload_rgba`, `src/renderer/renderer_init_methods.rs:43`, и блендинг в `draw_texture_quad`, `src/render_view/ui.rs:483`; записать в отчёт).
- `Mermaid`: mmdr в SVG-строку (ошибка разбора → `Mermaid(текст)`), шрифты mmdr — те же встроенные, если API позволяет; тёмная тема, если у mmdr есть пресет или настройка цветов (текст светлый, линии серые, фон прозрачный); если нет — стандартная тема и подложка: скруглённый светлый прямоугольник под диаграммой при растеризации; затем как `Svg`. Выбранный API mmdr и тему записать в отчёт.
- Размер растра (одна функция для всех видов): `display = natural * scale`; шире `max_raster_w` → пропорционально до `max_raster_w`; длинная сторона ≤ 4096; округление до целых ≥ 1; растр уменьшается `image::imageops::resize` (`Triangle`) только если целевой меньше исходного, SVG растеризуется сразу в целевой размер. `stamp` в результате — `None` (его ставит загрузчик).
- Реестр crate'ов в песочнице может быть пуст: API `image`/`usvg`/mmdr implementer сверяет по docs.rs-версиям из `Cargo.lock` до написания тестов; сомнение в API — вопрос в отчёте, не догадка.

- [ ] **Step 1:** Тесты (сначала красные), через `decode_in_process`: PNG 3×2 → natural 3×2, `scale 2.0` → raster 6×4; `max_raster_w` уменьшает пропорционально; JPEG, WebP, GIF (кодировать в тесте `image`-энкодером, где он есть; иначе маленькие байты-фикстуры в тесте); SVG `<svg width="10" height="20">` → natural 10×20; SVG с `<text>Build</text>` даёт непрозрачные пиксели в области текста; SVG с `<image href="http://127.0.0.1:<port>/x.png">` — сервер на порту 0 не получает ни одного соединения; альфа: полупрозрачный SVG-пиксель после конвертации — straight (значения цвета не затемнены); Mermaid `graph TD; A-->B` → ненулевой размер. Негативные: мусорные байты как `Raster` → `Decode`; PNG-заголовок 100000×100000 (IHDR с верным CRC, собрать вручную) → `TooManyPixels` без выделения памяти под картинку; невалидный и пустой Mermaid → `Mermaid(_)` с непустым текстом.
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_media::decode`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: безопасность декодеров (лимиты, запрет внешних ресурсов SVG), формат альфы для GPU.

### Task 3: Хелпер-процесс: протокол, запуск, `load_media`

**Files:**
- Create: `src/markdown_media/render_helper.rs` (протокол, `RenderCommand::for_current_process`, `render_media`, `run_media_helper_if_requested`)
- Modify: `src/markdown_media.rs` (`mod render_helper;`), `src/markdown_media/fetch.rs` (`load_media`), `src/main.rs` (ранний выход хелпера), `PROJECT_GUIDE.md` §4
- Test: `#[cfg(test)] mod tests` в `render_helper.rs`

**Interfaces:** Consumes — типы T1, `fetch_bytes`, `decode_in_process` (T2). Produces — `for_current_process`, `render_media`, `run_media_helper_if_requested`, `load_media`.

Решения (спека §2.3, §4.2):
- Протокол. stdin: строка `<kind> <scale> <max_raster_w> <len>\n` (`kind` = `raster|svg|mermaid`) и ровно `len` байт. stdout при успехе: строка `OK <natural_w> <natural_h> <raster_w> <raster_h>\n` и `raster_w*raster_h*4` байт RGBA8 straight alpha; при обработанной ошибке: одна строка `ERR <code> <текст до 200 символов>\n` (`code` = имя варианта `MediaError`), код выхода 0. Любой иной исход (ненулевой код, сигнал/abort, обрыв или несоответствие длины) → `Crashed`, таймаут → `Timeout`. Вход длиннее 20 МБ → `ERR TooLarge`.
- `run_media_helper_if_requested`: `std::env::args().nth(1) == Some("--rriter-media-render")` → читает запрос, вызывает `decode_in_process`, пишет ответ, возвращает `Some(0)`; ошибка чтения stdin → `Some(2)`. Иначе `None`. В `main.rs` ранний выход регистрируется так же, как хелпер повышенного сохранения: через `platform::handle_startup_helper` (`src/main.rs:134`, `src/platform/elevated_save.rs:190`), а не через константу флага (`elevated_save.rs:23` — только `ELEVATED_SAVE_FLAG`). Выход — до логов и окна.
- `render_media(Process{exe, prefix_args})`: `Command::new(exe).args(prefix_args).arg("--rriter-media-render")` через `platform::ManagedChild` (`take_stdin`/`take_stdout`/`wait_timeout`/`terminate`, `src/platform/process.rs:235-271`), процесс в группе процессов редактора, таймаут → завершение дерева процессов и `Timeout`. Запрос пишется в stdin из отдельного потока, пока основной поток читает stdout (вход до 20 МБ, выход до 64 МБ — иначе взаимная блокировка на заполненных pipe). `EPIPE` при записи сам по себе не ошибка: хелпер мог завершиться раньше, чем дочитал вход; исход решают код выхода и stdout. stdout больше `4096*4096*4 + 1 КБ` → `Crashed`. `InProcess` → `decode_in_process` напрямую.
- `for_current_process`: `cfg(test)` → `Some(InProcess)` (в тестовом бинарнике `current_exe` — не rriter); иначе `std::env::current_exe().ok().map(|exe| Process { exe, prefix_args: vec![] })`. `None` попадает в `FetchEnv.render`; его обрабатывает Task 4.
- `load_media(req, env)`: `env.render == None` → `Unsupported`; иначе `fetch_bytes`, затем `render_media` с таймаутом 10 с, `stamp` из `fetch_bytes` копируется в результат.
- Замера стоимости запуска в этой задаче нет (release-сборка implementer'у запрещена); замер — финальный шаг Task 10 у контроллера.

- [ ] **Step 1:** Тесты (сначала красные). Протокол (unix, `cfg(unix)`): `Process { exe: "sh", prefix_args: ["-c", "<скрипт>", "sh"] }` — скрипт `exit 101` → `Crashed` (и запись большого входа в stdin не зависает и не считается ошибкой сама по себе); `sleep 5` при таймауте 300 мс → `Timeout` и возврат < 2 с; `printf 'ERR Decode bad\n'` → `Decode`; `printf 'OK 1 1 1 1\n'` без пикселей → `Crashed`; `OK`-ответ с пикселями → `MediaPixels`; вход 20 МБ + 1 → `TooLarge`; разбор/сериализация протокола в обе стороны; `load_media` с `render: None` → `Unsupported`; `load_media` файла с `InProcess` — PNG на диске → пиксели и `stamp`.
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_media`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: изоляция процессов (таймаут, завершение дерева, отсутствие взаимной блокировки pipe), разбор недоверенного вывода.

### Task 4: Медиа-кэш `MarkdownMedia`

**Files:**
- Modify: `src/markdown_media.rs` (тип `MarkdownMedia`, записи, очередь, слоты, бюджет текстур, инвалидация, ревалидация, обрезка дискового кэша); если файл > 600 строк — бюджет и решения очереди в `src/markdown_media/texture_budget.rs`; `PROJECT_GUIDE.md` §4
- Test: `src/markdown_media/tests.rs` (`#[cfg(test)]`)

**Interfaces:** Consumes — `load_media`, `FetchEnv`, `trim_disk_cache`, `RenderCommand::for_current_process`, `UiWaker::spawn_one_shot` / `OneShot::poll` (`src/ui_waker.rs:138-190`), `Renderer::upload_rgba` / `delete_texture` (`src/renderer/renderer_init_methods.rs:43,61`). Produces — `MarkdownMedia` API, `MediaEntryView`, `VisibleMedia`, `MediaStats`.

Решения (спека §2.2, §4.1–4.4):
- `new(env)`: `MarkdownMedia` сам строит `FetchEnv` из платформенных частей — `http` из `platform::blocking_http_client_builder` с таймаутом 15 с (один клиент на кэш), `render` из `for_current_process()`. `render == None` → каждый запрос сразу `Failed(Unsupported)` (это владеет `MarkdownMedia`, не загрузчик).
- Запись: `natural: Option<(f32, f32)>`, `state: Pending | Ready | Failed(MediaError)`, `texture: Option<(glow::Texture, u32, u32)>`, `last_visible_frame: u64`, `in_flight: bool`, `generation: u64`, `stamp: Option<FileStamp>`, последний `MediaRequest` (для перерендера).
- `request`: неизвестный ключ → запись `Pending`, в очередь; известный → ничего, кроме «нет текстуры, не в полёте, не `Failed`» → в очередь (повторный рендер вытесненного). Очередь FIFO; ключи, видимые в последнем `prepare_gpu`, стартуют первыми.
- Слоты: ≤ 3 задач `OneShot` (`load_media` внутри); слот занят до загрузки результата на GPU или отбрасывания. `poll`: `Ok(px)` → натуральный размер (изменился → `media_gen += 1`), `stamp` записи обновляется, пиксели в «ожидают загрузки»; `Err(e)` → `Failed(e)`, `media_gen += 1`, слот свободен; `Closed` → `Failed(Crashed)`, слот свободен; затем старт задач из очереди. Возвращает `true`, если `media_gen` изменился. Каждая задача несёт `generation` записи на момент старта; пришедший результат с устаревшим `generation` отбрасывается (слот освобождается, `media_gen` не растёт).
- Решение «ставить ли ключ в очередь» (видим, нет текстуры, не в полёте, не `Failed`), «нужен ли перерендер» и бюджет вытеснения — чистые функции над срезами записей; `prepare_gpu` только применяет их результат. Так тесты вытеснения и повторной очереди идут без GL.
- `prepare_gpu`: первым делом освобождает текстуры из списка к освобождению (`delete_texture`) — так же, как PDF освобождает `pdf_textures_to_free` в `pdf_prepare_frame` (`src/app/pdf_tab/engine.rs:269`); загружает ≤ 2 ожидающих (видимые первыми), освобождая слоты и CPU-пиксели; `upload_rgba` вернул `None` → запись `Failed(TooManyPixels)`, `media_gen += 1`; для видимых с текстурой, где `|raster_w − display_w| / display_w > 0.25`, — перерендер с `max_raster_w = display_w` (старая текстура до замены); бюджет: невидимые текстуры сверх 128 МБ вытесняются по возрастанию `last_visible_frame` (их текстуры — в список к освобождению), видимые — никогда. Возвращает `true`, если есть ожидающие загрузки или видимые в полёте.
- `release_all` нет: хука уничтожения GL-контекста в приложении не существует; текстуры освобождаются только через список к освобождению.
- `invalidate_path`: запись `File(path)` удаляется (текстура — в список к освобождению), `media_gen += 1`; если по записи идёт задача, `generation` записи растёт, и устаревший результат отбрасывается при приходе (запись не воскресает).
- `revalidate_files(keys, waker)`: одна фоновая `OneShot`-задача (вне слотов) делает `metadata` для путей всех `File`-ключей из `keys`, сравнивает с сохранённым `FileStamp` записи; `poll` забирает результат и вызывает `invalidate_path` для изменившихся и исчезнувших. Записи без `stamp` (ещё не загружены, `Failed`) не сравниваются. Новая ревалидация при незавершённой предыдущей — не стартует (флаг).
- `stats()`: `loads_started` — счётчик запущенных `load_media`-задач за жизнь кэша; `texture_bytes` — сумма `w*h*4` живых текстур; `visible_texture_bytes` — то же по видимым в последнем `prepare_gpu`.
- `reset_failed`: убирает `Failed` из списка.
- Дисковый кэш: при первом запросе `Url` за сессию и далее на каждый 10-й запрос `Url` (≤ 10 × 20 МБ роста между обрезками) — фоновая `trim_disk_cache(cache_dir, 200 МБ)` через `spawn_one_shot` вне слотов.

- [ ] **Step 1:** Тесты на подменном загрузчике (`with_loader`; задачи — настоящий `spawn_one_shot` с `UiWaker::counting()`, `src/ui_waker.rs:50`; ждать `poll` в цикле с таймаутом 2 с): дедупликация (два `request` одного ключа → загрузчик вызван один раз, `stats().loads_started == 1`); ≤ 3 одновременно (загрузчик ждёт на `Barrier`/канале, 5 запросов → 3 вызова до освобождения); `media_gen` растёт при новом натуральном размере и при `Failed`, не растёт при повторном результате того же размера; `Failed` не перезапрашивается, `reset_failed` снимает; `invalidate_path` снимает запись и поднимает `media_gen`; `invalidate_path` при задаче в полёте → результат отброшен, записи нет; загрузчик, возвращающий `Err(Crashed)` (паника в тестах `panic-abort-tests` убила бы процесс) → `Failed(Crashed)`; `revalidate_files`: файл в tempdir изменён (другие mtime/len) → после `poll` запись сброшена; файл удалён → сброшена; не менялся → остаётся; `render == None` → `Failed(Unsupported)`. Чистые функции без GL: видимые сверх бюджета не вытесняются; невидимые — от старых к новым до бюджета; вытесненный и снова видимый ключ ставится в очередь ровно один раз за 5 циклов решений; перерендер при расхождении 30% и не при 20%; `stats()` считает байты текстур по срезу.
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_media`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: инварианты бюджета и слотов (нет циклов перерендера, нет утечки GL-текстур, ≤ 3 процессов), устаревшие результаты после инвалидации, интерфейс потребляют T6–T7.

### Task 5: Навигационная модель: заголовки, slug'и, цели ссылок, медиа-абзацы

**Files:**
- Modify: `src/languages/markdown.rs` (новые `MarkdownHeading`, `headings`, `link_definitions`; метка reference-ссылок уже есть как `reference_range` в `Link`/`Image`, `:118-136`, `:536-548`, — её только используют)
- Create: `src/app/markdown_nav.rs` (`heading_slugs`, `LinkTarget`, `resolve_link`, `MediaItem`, `media_paragraph`, `mermaid_item`; `LinkAction` добавит Task 8, попап — Task 9)
- Modify: `src/app.rs` (`mod markdown_nav;` рядом с `mod markdown;`), `PROJECT_GUIDE.md` §4
- Test: `#[cfg(test)] mod tests` в `markdown_nav.rs`; тесты в `src/languages/markdown.rs` рядом с существующими (`:930+`)

**Interfaces:** Consumes — `MediaKey`, `MediaSource` (T1), `PathKey`. Produces — всё T5 из таблицы.

Решения (спека §5.1, §6.1, §6.2):
- `heading_slugs`: нижний регистр (`char::to_lowercase`); сохраняются `char::is_alphanumeric()` (кириллица тоже), `-`, `_`; пробел → `-`; прочее удаляется; повтор → `-1`, `-2`… по порядку; если суффиксный slug уже выдан (заголовок `Intro 1` после двух `Intro`), следующий берёт первый свободный суффикс — все slug'и уникальны, первый в документе побеждает.
- `resolve_link(dest, doc_dir, defs)`: `[ref]`-форма → destination из `defs` (метка: нижний регистр, пробельные последовательности → один пробел, обрезка); нет определения → `Unsupported`. Затем: `http`/`https`/`mailto` → `External(dest)`; любая другая схема (`data:`, `file:`, `javascript:` …) → `Unsupported`; `#frag` → `Anchor(percent_decode(frag))`; иначе `path[#frag]`: percent-decode обоих, `File { path: doc_dir.join(path), anchor }`. Пустой dest → `Unsupported`. Percent-decode — своя функция (без новых crate), невалидные последовательности и не-UTF-8 результат остаются как исходный текст.
- `media_paragraph`: блок `Paragraph`, все инлайны которого — `Image`, `Link`, содержащий только `Image`, пробелы и переводы строк → `Some(items)`; иначе `None`. Локальный путь → `MediaKey::File(PathKey::new(&path))` (`src/platform.rs:410`; `From` нет) + `MediaSource::File(path)`; `http(s)` → `Url`; прочее (`data:` и т.п.) → `MediaSource::Url(dest)` (`fetch_bytes` вернёт `Unsupported`, рамка появится без спецслучая). `link` — `resolve_link` обёртки.
- `mermaid_item`: ключ `Mermaid(hash(code))` (std `DefaultHasher`: ключ живёт только в памяти процесса), `alt = "mermaid"`.
- `headings` и `link_definitions` — новые методы `MarkdownDocument`; есть ли в tree-sitter-md узлы определений ссылок, implementer проверяет на тесте и, если нет, разбирает определения по строкам исходника.

- [ ] **Step 1:** Тесты: slug'и (`"Привет, мир!"` → `привет-мир`, `"API v2.0"` → `api-v20`, два `"Intro"` → `intro`, `intro-1`, затем `"Intro 1"` → уникальный); `resolve_link` для каждого варианта + `%20` в пути и фрагменте, `other.md#Раздел`, `[text][Ref]` при `[ref]: x.md`, неизвестная метка, `javascript:alert(1)` → `Unsupported`, `%ZZ` остаётся как есть; `media_paragraph`: один `![a](x.png)`; ряд из трёх бейджей `[![b](https://img.shields.io/x.svg)](https://ci)` через переводы строк; абзац «текст ![a](x.png)» → `None`; `headings` с ATX- и setext-заголовками, с инлайн-разметкой внутри (текст без `*`/`` ` ``); `link_definitions`.
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_nav`, затем `make test TEST_FILTER=languages::markdown`.

**Агенты:** implementer ds-low-agentic · reviewer нет

### Task 6: Медиа в Read-режиме: раскладка и отрисовка

**Files:**
- Create: `src/render_view/markdown_read_media.rs` (чистая функция раскладки над `(natural, state)` элементов и шириной колонки, `PlacedMedia` без поля `link`, отрисовка текстуры/заглушки/рамки ошибки), `src/render_view/markdown_read_media_tests.rs` (точное имя файла тестов)
- Modify: `src/render_view/markdown_read.rs` (`ReadBlockKind::Media`; кэш раскладки хранит `media_gen`, с которым построен; `prepare_markdown_read_layout` `:899` принимает `&MarkdownMedia` и пересобирает раскладку при несовпадении `media_gen`; draw `:941`; ветка в `draw_markdown_block` `:1145`; `build_read_source_indices`; `MarkdownReadLayoutCache::media_blocks()` `pub(crate)`; бюджет ≤ +50 строк), `src/render_view/markdown_read_text_layout.rs` (абзац-медиа не идёт в текстовую вёрстку; блок mermaid), `src/render_view/markdown_read_interaction.rs` (медиа-блок ведёт себя как `Rule` — невыделяемый, копирование пропускает; `build_test_markdown_read_layout` `:1000` получает медиа-параметр), все прочие `match` по `ReadBlockKind`: `src/render_view/markdown_code_scroll.rs`, `src/render_view/markdown_scroll.rs` (включая обёртку `:585/610`, через которую проходит новый параметр), `PROJECT_GUIDE.md` §4
- Test: `src/render_view/markdown_read_media_tests.rs`

**Interfaces:** Consumes — `MarkdownMedia`, `MediaEntryView`, `VisibleMedia` (T4), `MediaItem` (T5). Produces — `ReadBlockKind::Media`, `PlacedMedia` (без `link`), функция раскладки, `media_blocks()`.

Решения (спека §4.3, §5):
- `LayoutKey` не меняется: у него около 32 мест вызова (`LayoutKey::new`, `is_valid_for_geometry`, включая тесты). Кэш раскладки сам хранит `media_gen`, с которым построен, а `prepare_markdown_read_layout` — единственное место, где он сравнивается с текущим и запускает пересборку. Если раскладку с новым `media_gen` строит проход, которого не касается привязка прокрутки, то привязку обеспечивает Task 7.
- Раскладка медиа-блока (§5.1–5.2): элементы в ряд слева направо, промежуток `(8.0 * s).round()`, перенос, высота строки ряда — по самому высокому; размер элемента: известен натуральный → `natural * scale`, шире колонки → пропорционально до ширины колонки, увеличения нет; неизвестен (`Pending` без размера, `Unknown`) → заглушка: ширина колонки × высота `line_h`; `Failed` → рамка шириной колонки, высота `line_h`, текст `alt — label()`. Вертикальные отступы блока — как у абзаца. Все координаты `.round()`. Функция раскладки не знает про `MarkdownMedia`: принимает срез `(natural, state)`; `MarkdownMedia` читает только тонкая обёртка в `markdown_read.rs`.
- Отрисовка: `Ready` с текстурой → `draw_texture_quad` в прямоугольник элемента; без текстуры → скруглённая рамка + alt (цвет и радиус — как у рамки блока кода); `Failed` → рамка + `alt — label()`. Только видимые блоки, scissor существующий. Mermaid-блок с `Failed(Mermaid(_))` — рисуется как сейчас код-блок плюс строка ошибки под шапкой (переиспользовать отрисовку код-блока, не копировать). Никакого I/O и запросов в отрисовке.
- Каждый существующий `match` по `ReadBlockKind` получает ветку `Media` (по смыслу как у `Rule`, если иное не нужно); перечень — по `rg 'ReadBlockKind' src/render_view`.

- [ ] **Step 1:** Юнит-тесты раскладки (чистая функция, без `MarkdownMedia`): одна картинка 400×300 при `scale 1.5` и колонке 800 → 600×450; при колонке 500 → 500×375; три бейджа 90×20 в колонке 200 → две строки ряда; `Pending` без размера → высота `line_h`; `Failed` → высота `line_h`; координаты целые при `scale 1.333`. Кэш: раскладка, построенная с `media_gen = 1`, при вызове с `media_gen = 2` пересобирается, при том же — нет; `media_blocks()` отдаёт ключи и прямоугольники.
- [ ] **Step 2:** Реализация до зелёного: `make test TEST_FILTER=markdown_read`, `make test TEST_FILTER=render_view::markdown`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: отрисовка без I/O, интерфейс раскладки потребляют T7–T8.

### Task 7: Проводка в App: запросы, кадр, привязка прокрутки, ревалидация, dump

**Files:**
- Modify: `src/app/app_state.rs` (одно поле `markdown_media: MarkdownMedia`), `src/app/markdown.rs` (запросы медиа после `refresh_read_model` `:148`), `src/app/events/main_frame.rs` (порядок кадра рядом с `pdf_prepare_frame`, вызов на `:123`), `src/render_view/markdown_scroll.rs` / `src/app/markdown_scroll_transition.rs` (только если привязка требует правки, см. ниже), `src/app/events/about/about_tick_scroll_sections.rs` (`about_to_wait_file_watcher` `:438`), `src/app/app_file_tab_methods.rs` (`open_file_in_tab` `:307`, переключение вкладки — активация md-вкладки), `src/headless/dump.rs` (вкладки `:253,:275,:346`; верхний уровень), `src/headless/mod.rs` (регистрация теста рядом с `:77`), `docs/headless.md` (поля dump), `PROJECT_GUIDE.md` §4
- Test: `src/render_view/markdown_read_media_tests.rs` (дополнить), `src/app/markdown_app_tests.rs`, `src/headless/ui_tests_markdown_media.rs`

**Interfaces:** Consumes — `MarkdownMedia`, `MediaStats`, `revalidate_files`, `reset_failed` (T4), `media_paragraph`, `mermaid_item` (T5), `ReadBlockKind::Media`, `media_blocks()`, `media_gen` кэша раскладки (T6), `ScrollState` (`src/scroll.rs`). Produces — проводка; поля dump `markdown_media` и `markdown_media_stats`.

Решения (спека §4.1, §4.3, §4.4, §5.4):
- Запросы: после каждого `refresh_read_model` App проходит блоки документа, собирает `media_paragraph`/`mermaid_item` и вызывает `markdown_media.request(MediaRequest { key, source, max_raster_w, scale })`, где `max_raster_w` — ширина колонки текста последней раскладки вкладки в пикселях (нет раскладки — ширина окна), `scale` — масштаб UI. Дедупликацию делает кэш.
- Порядок кадра (спека §4.3), в `main_frame.rs` рядом с `pdf_prepare_frame`, до отрисовки markdown: `poll` → раскладка активной вкладки с `media_gen` → видимые медиа из видимого диапазона блоков (`media_blocks()`) → `prepare_gpu` → при `true` запрос следующего кадра тем же способом, каким PDF просит перерисовку (`window.request_redraw()`, `src/app/pdf_tab/engine.rs:302`). В headless-приложении `for_current_process()` даёт `InProcess` (`cfg(test)`) или процесс test-бинарника; фикстуры headless-тестов не зависят от хелпер-процесса.
- Привязка прокрутки при смене `media_gen`: использовать существующий путь раскладки с сохранением владения текущей позицией — `prepare_markdown_read_layout_preserving_current_ownership` (`src/render_view/markdown_scroll.rs:585`; уже держит привязку к исходному якорю при смене ширины/шрифта, см. также `src/app/markdown_scroll_transition.rs:585-607`). Implementer первым делом выясняет, срабатывает ли этот путь при пересборке из-за `media_gen`: если да — только убедиться тестами; если нет — после пересборки применить к `ScrollState` `rebase_current_preserving_motion` (`src/scroll.rs:226`): он сдвигает и текущую позицию, и цель анимации на одну дельту положения верхнего видимого блока (`source_range`). `rebase_current_preserving_target` (`src/scroll.rs:244`) не подходит: он оставляет цель абсолютной. `jump_to` не использовать (он сбрасывает движение). Границы прокрутки обновляются (`set_read_scroll_bounds`). Переход Read/Edit (`markdown_scroll_transition.rs`): проверить, берёт ли он геометрию из текущей раскладки по `source_range` на каждом шаге; если кэширует y — пересчитывать при смене `media_gen`. Решение записать в отчёт.
- Ревалидация (спека §4.4): `revalidate_files(File-ключи активного документа, waker)` вызывается на трёх триггерах — событие вотчера в `about_to_wait_file_watcher` (оно без путей: `src/app/file_tree_scan.rs:542,568`), активация md-вкладки и переоткрытие. Переоткрытие дополнительно делает `reset_failed(ключи документа)`. Вотчер по путям не меняется.
- Dump: `src/headless/dump.rs` — у каждой вкладки `markdown_media: [{key, state: pending|ready|failed:<Code>, x, y, w, h}]` (из `media_blocks()` и `entry`; `<Code>` — `Debug`-имя варианта `MediaError` без полей; только у вкладок с раскладкой, иначе пустой массив) и верхнеуровневый `markdown_media_stats` из `stats()`. Новый файл headless-тестов `ui_tests_markdown_media.rs` регистрируется в `src/headless/mod.rs` рядом со строкой 77.

- [ ] **Step 1:** Юнит-тесты привязки прокрутки (`markdown_read_media_tests.rs`, `markdown_app_tests.rs`): в покое — блок под верхом экрана после роста картинки выше него остаётся на том же экранном y; картинка ниже экрана выросла — прокрутка не меняется; во время анимации колеса к цели T картинка выше выросла на D — цель стала T + D, скорость сохранена; переход к якорю (`animate_to` к заголовку) при росте картинки выше заголовка доезжает до заголовка; во время перехода Read/Edit — итоговая позиция соответствует тому же `source_range`.
- [ ] **Step 2:** Headless-тест (`ui_tests_markdown_media.rs`, фикстуры пишет сам тест в `scratch_dir`): `.md` с локальным PNG 64×32 (сгенерировать `image`), бейджем-SVG и отсутствующим `missing.png` → после `wait_until` в dump: PNG `ready` с `w/h` = 64×32 × scale (scale брать из dump, если он там есть), SVG `ready`, `missing.png` `failed:NotFound`; изменить PNG на 32×32 → триггер ревалидации (переключение вкладки туда-обратно или tick вотчера) → `wait_until` новый размер; удалить PNG → `failed:NotFound`; блок ```mermaid с `graph TD; A-->B` → `ready`; с мусором → `failed:Mermaid`. Прямую подстановку данных в обход UI в тесте указать в отчёте.
- [ ] **Step 3:** Реализация до зелёного: `make test TEST_FILTER=markdown`, `make test TEST_FILTER=headless::ui_tests_markdown`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: порядок кадра, инварианты прокрутки между раскладкой, `ScrollState` и переходом Read/Edit.

### Task 8: Кликабельные ссылки

**Files:**
- Modify: `src/render_view/markdown_read.rs` (`StyledRun.link` `:139-144`, `links: Vec<LinkTarget>` в `MarkdownReadLayoutCache`; бюджет ≤ +40 строк), `src/render_view/markdown_read_text_layout.rs` (`append_inline` присваивает индекс ссылки всем run'ам её текста, включая перенесённые строки и вложенные стили), `src/render_view/markdown_read_media.rs` (добавляет `PlacedMedia.link: Option<u32>` и заполняет его из `MediaItem.link`), `src/render_view/markdown_read_interaction.rs` (`link_at`), `src/app/markdown_nav.rs` (`LinkAction`, `link_action`), `src/app/markdown.rs` (`pending_anchor`, применение после готовой раскладки), `src/app/mouse/input/mouse_ui_element_press.rs` (`:41-50`, нажатие в `MarkdownReadBody`) и обработчик отпускания мыши (найти место завершения жеста выделения, например `finish_read_selection_gesture`), `src/app/events.rs` (`markdown_read_cursor_icon` — рука над ссылкой), `src/platform/desktop_request.rs:210` (`open_url` принимает `mailto:`), подсказка — `Renderer::delayed_tooltip_anchor` (`src/renderer/renderer_init_methods.rs:66`, образец использования `src/render_view/tabs_ui.rs:714-723`), `PROJECT_GUIDE.md` §4 при новых файлах
- Test: `src/app/markdown_nav.rs` (tests), `src/render_view/markdown_read_interaction_tests.rs`, `src/headless/ui_tests_markdown_media.rs`

**Interfaces:** Consumes — `LinkTarget`, `resolve_link`, `heading_slugs`, `MarkdownDocument::headings`/`link_definitions` (T5), `PlacedMedia`, раскладка медиа (T6). Produces — `links`, `link_at`, `PlacedMedia.link`, `LinkAction`, `link_action`, `pending_anchor`.

Решения (спека §6.3):
- Бюджет строк: `markdown_read.rs` растёт здесь не больше чем на 40 строк; если не помещается (или Task 6 вышла за свои +50), единица поведения (хранение и поиск ссылок раскладки) выносится в отдельный файл в этой же задаче.
- `PlacedMedia.link` — индекс в `links` раскладки; медиа-раскладка получает `LinkTarget` элементов и регистрирует их в том же `links`, что и текстовые ссылки.
- `link_action`: `External(u)` → `OpenUrl(u)`; `Anchor(s)` → заголовок со slug `s` → `ScrollTo(source_range)`, нет → `None`; `File { path, anchor }`: расширение `.md`/`.markdown` (без учёта регистра) → `OpenMarkdown`, иначе `OpenFile`; `Unsupported` → `None`.
- Клик: при нажатии в `MarkdownReadBody` запомнить `link_at(x, y)` и точку; выделение начинается как сейчас. При отпускании: смещение ≤ `(4.0 * s).round()` px по обеим осям и `link_at` = та же ссылка → выделение сбросить и выполнить действие; иначе — обычное выделение. Картинка-ссылка (`PlacedMedia.link`) — та же логика.
- Действия App: `OpenUrl` → `platform::open_url(self.external_requests.sink(), &url)` (образец `src/app/pdf_tab/input.rs:135`), ошибка — в существующий показ ошибок; `ScrollTo` → `scroll_y.animate_to(source_target_y)`; `OpenMarkdown` → `open_file_in_tab(path, true)`, режим Read (`set_markdown_mode`), `pending_anchor = anchor`; `OpenFile` → `open_file_in_tab(path, true)`; отсутствующий файл — как при обычном открытии отсутствующего файла.
- `pending_anchor` применяется в начале кадра, когда у вкладки есть раскладка с `content_height() > 0`: найти заголовок, `jump_to` (новая вкладка — без анимации), очистить; нет заголовка — очистить.
- Hover: над ссылкой курсор `Hand`, подсказка — текст destination (для `File` — путь как в исходнике) через `Renderer::delayed_tooltip_anchor` с его задержкой.

- [ ] **Step 1:** Юнит: `link_action` для всех вариантов; вёрстка присваивает один индекс всем run'ам ссылки `[**жирный** текст](x.md)`, перенесённой на две строки; `link_at` попадает в оба куска и не попадает в текст рядом; бейдж-ссылка в медиа-блоке получает `link`.
- [ ] **Step 2:** Headless: `a.md` со ссылками `[to b](b.md#Раздел-2)`, `[top](#заголовок)`, `[img](./pic.png)`; клик по первой → вкладка `b.md` активна, режим Read, после `wait_until` прокрутка у заголовка `Раздел 2`; клик по второй в длинном `a.md` → `scroll_y` доезжает до заголовка; клик по третьей → открыта обычная вкладка `pic.png` (или то, что сейчас делает `open_file_in_tab` для png — записать); перетаскивание от ссылки на 40 px → выделение непустое, новых вкладок нет; `https://`-ссылка → запрос в `ExternalRequestSink` (`open_url` в headless перехватывается: `src/platform/desktop_request.rs:210-212`; проверить через тестовый приёмник, как делают существующие тесты, если есть; иначе юнит на уровне `link_action`).
- [ ] **Step 3:** Реализация до зелёного: `make test TEST_FILTER=markdown`, `make test TEST_FILTER=headless::ui_tests_markdown`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: изменение поведения мыши в Read (клик против выделения), маршрутизация внешних действий.

### Task 9: Оглавление

**Files:**
- Modify: `src/app/markdown_nav.rs` (`TocPopup` — состояние попапа: открыт, выбранная строка, прокрутка; построение списка из `headings`), `src/app/app_state.rs` (поле `markdown_toc: TocPopup`, одно), `src/ui_system/ui_ids.rs` (`MarkdownTocToggle`, `MarkdownTocItem(usize)`), кнопка рядом с `MarkdownModeToggle`: отрисовка — `src/render_view/ide_panels/ide_panel_dialog_renderer.rs:440`, клик — `src/app/ui_handlers/ui_panels.rs:9`, hit-тест — `src/app/mouse/input.rs:245`, `src/app/keyboard/editor_keys.rs` (сочетание до ветки `KeyO if ctrl` `:776`), `src/app/keyboard/main_keys.rs` (стрелки/Enter/Escape при открытом попапе — по образцу автодополнения), рендер попапа (`draw_animated_context_menu`, `ide_panel_dialog_renderer.rs:951`, если умеет прокрутку; иначе новый рендер в `src/render_view/markdown_toc.rs` ≥ 200 строк на общем виджете скроллбара), колесо над попапом (`src/app/mouse/wheel.rs`), `src/headless/dump.rs` (верхнеуровневый `markdown_toc: {open, items, selected}`), `src/headless/mod.rs` (регистрация `ui_tests_markdown_toc.rs` рядом с `:77`), `PROJECT_GUIDE.md` §4
- Test: `src/app/markdown_nav.rs` (tests), `src/headless/ui_tests_markdown_toc.rs`

**Interfaces:** Consumes — `MarkdownDocument::headings` (T5), `ScrollTo`-путь прокрутки (T8), `refresh_markdown_read_model_if_stale` (`src/app/markdown.rs:490`). Produces — `TocPopup`, поле dump `markdown_toc`.

Решения (спека §7):
- Открытие: кнопка или `Ctrl+Shift+O` (на macOS основной модификатор приложения — `Cmd`; взять тот же хелпер модификатора, что у соседних сочетаний) только во вкладке markdown; в остальных вкладках сочетание проходит в существующую ветку без изменений. Перед открытием — `refresh_markdown_read_model_if_stale` (в Edit модель сама не обновляется).
- Строки: текст заголовка, отступ `((level - 1) as f32 * 12.0 * s).round()`, обрезка многоточием по ширине попапа; выбранная строка подсвечена; ширина попапа — `(360.0 * s).round()`, высота — до 60% окна, дальше прокрутка колесом и скроллбар.
- Клавиатура при открытом попапе: `↑`/`↓` — выбор с прокруткой к выбранной, `Enter` — переход и закрытие, `Escape` — закрытие; клик вне — закрытие; клавиши не уходят в редактор, пока попап открыт.
- Переход: Read — `animate_to` к `source_target_y(heading.source_range)`; Edit — курсор редактора на строку начала заголовка и прокрутка к ней (существующий переход к строке — найти, как это делает поиск или go-to-definition).

- [ ] **Step 1:** Юнит: список строк из заголовков (уровни, отступы, пустой документ → попап не открывается / пустой список с текстом «нет заголовков» — выбрать и записать); навигация `↑`/`↓` на краях не выходит за список.
- [ ] **Step 2:** Headless: markdown-вкладка, Read: `ctrl+shift+o` → в dump `markdown_toc.open`, N строк; `down`, `enter` → попап закрыт, `scroll_y` доезжает до второго заголовка; Edit: изменить текст заголовка (`type`), `ctrl+shift+o` → в dump новый текст, `enter` → строка курсора = строка заголовка; `escape` закрывает; в `.rs`-вкладке `ctrl+shift+o` — dump до/после совпадает с поведением до изменения (сначала снять поведение на release-бинарнике и записать в тест); `ctrl+o` в markdown-вкладке делает то же, что раньше.
- [ ] **Step 3:** Реализация до зелёного: `make test TEST_FILTER=markdown_nav`, `make test TEST_FILTER=headless::ui_tests_markdown_toc`.

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: изменение маршрутизации клавиш (перехват `Ctrl+Shift+O` до `Ctrl+O`, захват клавиш попапом).

### Task 10: Сквозная проверка, нагрузка, документация, замер хелпера

**Files:**
- Test: `src/headless/ui_tests_markdown_media.rs` (дополнить)
- Modify: `docs/headless.md` (если какие-то поля dump не описаны предыдущими задачами). `PROJECT_GUIDE.md` §4 здесь не трогается: каждая задача внесла свои файлы сама

**Interfaces:** Consumes — всё выше, в том числе `markdown_media_stats` из dump (T7).

- [ ] **Step 1:** Headless-фикстура с http: сервер на `127.0.0.1:0` в тесте отдаёт PNG и SVG-бейдж; `.md` с ними, локальным PNG, Mermaid и ссылками → все медиа `ready`; сервер отдаёт 404 для одного URL → `failed:Http`; повторное открытие вкладки после «починки» сервера → `ready` (сброс `Failed`).
- [ ] **Step 2:** Нагрузка: `.md` с 30 PNG 1600×1200 (сгенерировать один раз в тесте, 30 разных путей-копий), прокрутка колесом вниз до конца и обратно ×2 (`wheel` + `settle`) → после `wait_until` все видимые `ready`; по `markdown_media_stats` из dump: `texture_bytes` ≤ 128 МБ + `visible_texture_bytes`; `loads_started` ≤ 30 + число вытесненных-и-вернувшихся (нет цикла); кадр не блокируется (время `settle` ≤ разумного порога — записать фактическое).
- [ ] **Step 3:** Один снимок 2560×1440 @ 1.333 фикстуры шага 1 в workspace плана (`.superpowers/sdd/2026-10-01-markdown-reader-media/markdown-media.png`) для визуальной приёмки контроллером (нужна release-сборка с фичей; собирает контроллер).
- [ ] **Step 4 (контроллер SDD, не implementer):** после финального `make codex_test` (он собирает release) замерить через настоящий хелпер один SVG-бейдж с текстом и один flowchart Mermaid (`graph TD; A-->B`): время `render_media(Process)` на элемент. Больше 50 мс на элемент → контроллер добавляет задачу на долгоживущий хелпер по спеке §2.3 (один хелпер на слот, завершение после 60 с простоя, упавший отдаёт `Failed` текущему элементу без повторов); иначе фиксирует цифры в ledger.

**Агенты:** implementer ds-low-agentic · reviewer нет
