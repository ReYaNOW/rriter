# Просмотр PDF — дизайн (v1)

Карточка Kanri: `vn2frxr48wa5inpi7lanrhuc` («добавить поддержку pdf», Features).
Handoff с историей решений: `~/.claude/handoffs/rriter-20260929-pdf.md`.

## 1. Цель и границы

Открывать `.pdf` из дерева файлов, drag-drop, CLI и сессии во вкладке-просмотрщике «уровня markdown-ридера»: непрерывная лента страниц по ширине окна, Ctrl+F с подсветкой и переходом по совпадениям, выделение мышью и Ctrl+C, кликабельные ссылки. Без редактирования. Минимум RAM, диска и размера бинаря: растеризуются только видимые страницы, кэш текстур ограничен, дискового кэша нет, библиотека движка не вшивается в бинарь.

**В v1:**
- вкладка `EditorTabKind::Pdf` с собственным рендером и маршрутизацией ввода;
- fit-width (одна ширина = ширина области вкладки минус поля), вертикальная лента страниц с зазорами, скроллбар, колесо через общие helper'ы (Ctrl = ускорение, как везде), PgUp/PgDn/Home/End/стрелки;
- индикатор «стр. N / M» в статус-баре;
- Ctrl+F: та же панель поиска, что у редактора; regex тот же; подсветка совпадений на видимых страницах; Enter/Shift+Enter — следующее/предыдущее с прокруткой;
- выделение текста мышью (в т.ч. через несколько страниц), Ctrl+C;
- ссылки: URI (http/https) → браузер, GoTo → прокрутка к странице/точке; курсор-рука над ссылкой;
- состояния вкладки: движок не установлен (кнопка «Загрузить»), загрузка, готово, ошибка файла, защищён паролем;
- поставка движка: поиск библиотеки рядом с exe / в data-dir / по переменной; загрузка одной версии из `pdfium-binaries` с проверкой SHA-256; `make pdfium`; build-скрипты win/mac кладут библиотеку в пакет;
- восстановление страницы и смещения из сессии;
- headless-`dump` и ~10 UI-сценариев на сгенерированной fixture (§10.3).

- тёмная тема: страницы растеризуются с цветовой схемой pdfium (тёмный фон, светлый текст и вектор, картинки как есть), переключатель «Тёмные страницы» на PDF-вкладке, по умолчанию следует теме редактора (§6.5).

**Не в v1:** zoom (решение пользователя: только fit-width), панель outline/закладок, запрос пароля, инверсия картинок/сканов в тёмной теме (§6.5), аннотации/формы, печать, Ctrl+G «перейти к странице» (в редакторе нет диалога go-to-line, заводить ради PDF не будем), Ctrl+A, контекстное меню, поворот страниц, миниатюры.

## 2. Ключевые решения

### 2.1 Движок: PDFium через `pdfium-render`, библиотека динамическая

Сравнение на одном железе и одном корпусе (15 PDF, 27 страниц, 144 dpi, эталон poppler), 29–30.09.2026:

| | stet-pdf-reader 0.8.2 (pure Rust) | pdfium (chromium/8066) |
|---|---|---|
| диск | +10,8 МиБ в бинаре | +2,7 МБ бинарь + 7,8 МБ `libpdfium` (архив 3,5–3,8 МБ/платформа) |
| RSS, медиана / макс | 60 / 378 МБ | 36 / 293 МБ |
| рендер страницы, медиана / макс | 25 / 255 мс | 10 / 98 мс |
| корректность | 3 дефекта: пропадает текст base-14 без FontDescriptor (при OTF Nimbus в системе), пустые названия outline (Ghostscript.pdf), не видит 76 ссылок на glm.pdf | совпало с poppler |
| устойчивость на мусоре | `Err`, без паник | `Err`, без паник/segfault |

По диску паритет, по RAM/скорости pdfium лучше, по корректности stet требует чинить чужой парсер. Выбран pdfium. Лицензии: pdfium BSD-3, обёртка MIT/Apache — совместимы с закрытой поставкой (у проекта лицензии нет).

Cargo: `pdfium-render = "0.9"` с `default-features = false, features = ["pdfium_latest"]` (без `static`, без `thread_safe` — доступ и так из одного потока, см. §4); `flate2`, `tar` (архивы `.tgz`), `sha2` (проверка хеша). Точные версии — на этапе плана по `cargo add --dry-run`.

### 2.2 Один фоновый поток-владелец движка

`pdfium-render` сериализует все вызовы, параллельные потоки ничего не дают. Один поток на приложение, стартует при первом открытии PDF, живёт до выхода, владеет `Pdfium` и всеми открытыми документами. UI общается с ним запросами/событиями через `UiWaker::channel`, дренаж в `about_to_wait` по образцу `tool_installer` (`src/app/events/about.rs:193`, `about_tick_input_sections.rs:335`).

### 2.3 Страница = одна GL-текстура

Воркер растеризует страницу целиком (текст, вектор, картинки, сканы) в RGBA по запрошенной ширине в пикселях; UI загружает битмап в текстуру один раз и рисует квадом. Растеризация вне кадра; в draw-path только bind + quad. Кэш текстур на вкладку — видимые страницы ±2.

### 2.4 Поставка: рядом с exe в пакетах, загрузка по требованию иначе

Release win/mac: библиотека в пакете, подписана build-скриптом. Linux и dev: загрузка в `data_dir()/tools/managed/pdfium/<version>/` при первом открытии PDF по кнопке, один раз на машину и версию. Архив в бинарь не вшивается (+3,7 МБ платили бы все, а сетевой путь для dev всё равно нужен). Единый источник версии и хешей — `pdfium.json` в корне репозитория.

### 2.5 Тесты — на наш шов, не на движок

pdfium отдельно не тестируем. Headless-сценарии на fixture через сам вьювер проверяют наш код: пересчёт координат, нормализацию текста, ссылки, состояния ошибок, совместимость версии библиотеки после обновления `pdfium.json`.

## 3. Модули и файлы

Новые файлы (ориентировочные размеры; лимиты проекта: мягкий 1500, жёсткий 1600 строк; новые файлы ≥ 200 строк):

| файл | назначение |
|---|---|
| `src/pdf/mod.rs` | публичные типы (§4.1), `PdfWorkerHandle`, `PdfError`; ничего из `pdfium_render` |
| `src/pdf/pdfium_backend.rs` | **единственный** файл с `use pdfium_render`: bind библиотеки, open, render, text+links, нормализация координат и текста |
| `src/pdf/worker.rs` | поток, очередь запросов, поколения, `catch_unwind`, отправка событий |
| `src/pdf/library.rs` | манифест `pdfium.json` (`include_str!`), ключ платформы, `locate()`, `managed_path()` |
| `src/app/pdf_tab.rs` | `PdfTabState`, `PdfPhase`, раскладка, прокрутка, применение событий воркера, очередь битмапов |
| `src/app/pdf_tab/text.rs` | кэш текста страниц, поиск (состояние на вкладке), выделение, копирование, hit-test ссылок |
| `src/app/keyboard/pdf_keys.rs` | клавиши вкладки PDF (`main_keys.rs` на жёстком лимите 1600 — в него только вызов) |
| `src/render_view/pdf_view.rs` | `draw_root_pdf_frame`: страницы, плейсхолдеры, подсветка, выделение, скроллбар, экраны состояний |
| `src/headless/ui_tests_pdf.rs` | fixture-генератор + сценарии (§10) |
| `pdfium.json` | версия, архивы и SHA-256 по платформам |
| `scripts/fetch_pdfium.py` | загрузка/проверка/распаковка по `pdfium.json`; используется `make pdfium`, `build_windows.py`, `build_macos.py` |

Правки существующих: `app_state.rs` (вариант `Pdf` в `EditorTabKind`, поле `pdf_worker`, `pdf_engine`), `app_ide_tab_methods.rs`/`app_window_external_methods.rs` (ветка `.pdf` в `open_file_in_tab` — одна строка-диспатч; файлы у лимита, логика в `pdf_tab.rs`), `root_frame_renderer.rs` (ранний `return` для `Pdf`), `mouse/wheel.rs`, `mouse/input.rs`, `events/main_frame.rs` (курсор), `app_file_tab_methods.rs` (`update_search`, `jump_to_search_result`, закрытие), `state_persistence.rs` (`PDF\t`), `renderer` (§6.2), `ui_ids.rs`, `ui_handlers/*`, `headless/dump.rs`, `Makefile`, `scripts/build_*.py`, `Cargo.toml`, `PROJECT_GUIDE.md` §4.

## 4. Движок и воркер

### 4.1 Типы (`src/pdf/mod.rs`)

```rust
pub struct DocId(pub u64);                       // выдаёт App, монотонно
pub struct PageGeom { pub width_pt: f32, pub height_pt: f32 }
pub struct PtRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }  // pt, origin — верхний левый угол страницы
pub struct PageChar { pub ch: char, pub rect: PtRect }               // '\n' — rect нулевой
pub struct PageText { pub chars: Vec<PageChar> }                     // текст = chars.iter().map(|c| c.ch)
pub enum LinkTarget { Uri(String), Page { page: usize, y_pt: Option<f32> } }
pub struct PageLink { pub rect: PtRect, pub target: LinkTarget }
pub enum PdfError { PasswordRequired, Invalid(String), Engine(String) }

pub struct DocGens {                              // Arc, общий для вкладки и воркера
    pub render: AtomicU32, pub search: AtomicU32, pub closed: AtomicBool,
    pub wanted: AtomicU64,                        // (first_page << 32) | last_page — окно «visible ± 2», обновляет вкладка
}

pub enum PdfRequest {
    Open { id: DocId, path: PathBuf, gens: Arc<DocGens> },
    Close { id: DocId },
    Render { id: DocId, page: usize, width_px: u32, gen: u32, dark: bool },   // dark — §6.5
    PageText { id: DocId, page: usize },
    Search { id: DocId, query: String, gen: u32 },
}
pub enum PdfEvent {
    EngineReady,
    EngineFailed(String),                       // bind не удался или поток упал
    Opened { id: DocId, pages: Vec<PageGeom> },
    OpenFailed { id: DocId, error: PdfError },
    Page { id: DocId, page: usize, gen: u32, width_px: u32, height_px: u32, rgba: Vec<u8> },
    RenderSkipped { id: DocId, page: usize, gen: u32, error: Option<String> }, // устарел / вне окна / ошибка pdfium
    Text { id: DocId, page: usize, text: Arc<PageText>, links: Vec<PageLink> },
    TextFailed { id: DocId, page: usize, error: String },
    SearchPage { id: DocId, gen: u32, page: usize, matches: Vec<(u32, u32)> },  // диапазоны индексов chars
    SearchDone { id: DocId, gen: u32 },
}
```

Контракт: на **каждый** `Render` приходит ровно одно из `Page`/`RenderSkipped`, на каждый `PageText` — `Text`/`TextFailed` (кроме случая `closed`, когда воркер молчит, а вкладка уже удалена). Вкладка снимает `(page, gen)` из `requested` и `text_requested[page]` по любому из них; после `TextFailed` страница помечается «без текста» (пустой `PageText`), повторно не запрашивается.

```rust
```

`PdfWorkerHandle { tx: Sender<PdfRequest>, rx: Receiver<PdfEvent> }` — создаётся `pdf::worker::start(lib_path: PathBuf, waker: &UiWaker)`; поток биндит библиотеку и первым шлёт `EngineReady`/`EngineFailed`.

### 4.2 Backend (`pdfium_backend.rs`)

- `bind(path) -> Result<Pdfium, String>` через `Pdfium::bind_to_library`.
- `open(&Pdfium, path) -> Result<Doc, PdfError>`: `PdfiumError::PdfiumLibraryInternalError(FormatError)` → `Invalid`, ошибка пароля → `PasswordRequired`, прочее → `Engine(msg)`. Размеры страниц читаются сразу (ленивые страницы, Open ≤ 2 мс даже на 477 стр.).
- `render(doc, page, width_px) -> (w, h, Vec<u8>)`: `height_px = round(width_px · h_pt / w_pt)`, RGBA8 без альфы (белый фон), без вращения. Битмап один на запрос, буфер отдаётся в событие (move), в воркере не кэшируется.
- `text(doc, page) -> (PageText, Vec<PageLink>)`: символы `PdfPageText::chars()` с bbox; **координаты**: pdfium даёт pt с origin внизу-слева — здесь переворачиваем `y' = height_pt − (y + h)`; выше backend'а перевёрнутых координат нет. **Нормализация**: `\r\n`/`\r` → `\n`; `\u{2}` (маркер переноса) выбрасывается; прочие управляющие < 0x20 кроме `\n` выбрасываются; лигатуры pdfium отдаёт разложенными — как есть. Ссылки: аннотации Link с действием URI (только `http://`/`https://`, остальные пропускаем) или GoTo/Dest с номером страницы и, если есть, верхней координатой (тоже перевёрнутой).
- Поиск делает воркер, не backend: строит `String` из chars, ищет regex (тот же `regex` crate и та же семантика, что в `update_search` редактора: флаги case/regex из панели передаются в запросе как готовый паттерн), байтовые смещения → индексы chars по `char_indices`.

### 4.3 Воркер (`worker.rs`)

- Обработка каждого запроса под `catch_unwind`; паника → `EngineFailed("…")`, поток завершается. **Перезапуска нет**: `pdfium-render` 0.9 при повторном `bind_to_library` в том же процессе возвращает `AlreadyInitialized` — движок остаётся `Failed` до перезапуска приложения, экран вкладки так и говорит («перезапустите RRiter»).
- Приоритет вместо чистого FIFO: цикл `try_recv` дренирует всё, что пришло; `Open`/`Close`/`Render`/`PageText` выполняются сразу, `Search` лишь заменяет `active_search` (новый `gen`); когда очередь пуста и есть `active_search` — ищется **одна** страница, затем снова `try_recv`; когда нет ни того ни другого — блокирующий `recv`. Так скролл и resize не ждут поиска по 477 страницам, а поиск не голодает при простое.
- `Render`: перед работой проверяет `gen == gens.render`, `!closed` и `page ∈ wanted ± 0` (окно уже включает ±2); не прошло — `RenderSkipped`. Быстрый скролл через сотни страниц не растеризует всё, что мелькнуло.
- `Search`: после каждой страницы проверяет `gens.search == gen` и `!closed`; иначе бросает без `SearchDone`. Текст берёт из собственного кэша `HashMap<(DocId, page), Arc<PageText>>` (тот же `Arc`, что уходит в `Text`).
- `Close` удаляет документ, его кэш и `active_search`. `closed` выставляет вкладка до отправки `Close`; события для закрытого `DocId` App игнорирует.

## 5. Вкладка

### 5.1 Открытие

`open_file_in_tab` (все входы: дерево `file_tree.rs:883`, drop `events.rs:524`, пикер, CLI, headless, сессия `app_window_external_methods.rs:1063-1080`) — до `platform::read_text_file` одна ветка: расширение `pdf` без учёта регистра → `self.open_pdf_tab(path)`. Дедуп по `PathKey` как у остальных вкладок: уже открыт → активировать.

`open_pdf_tab`: если `pdf_engine` не `Ready` — вкладка создаётся с `phase = EngineMissing` (или `EngineStarting`, если библиотека найдена и воркер поднимается); иначе `Open` в воркер, `phase = Loading`. Заголовок вкладки — имя файла, иконка PDF.

### 5.2 Состояние

```rust
pub struct PdfTabState {
    pub path: PathBuf, pub doc: Option<DocId>, pub gens: Arc<DocGens>,
    pub phase: PdfPhase,
    pub pages: Vec<PageGeom>, pub layout: PdfLayout,       // §6.1
    pub scroll: ScrollState,
    pub textures: HashMap<usize, PageTexture>,              // §6.3
    pub pending_bitmaps: Vec<PendingBitmap>,                // пришли из воркера, ждут загрузки в GPU
    pub requested: HashSet<(usize, u32)>,                   // (page, gen) — рендер в полёте
    pub text: Vec<Option<Arc<PageText>>>, pub links: Vec<Vec<PageLink>>, pub text_requested: Vec<bool>,
    pub search: PdfSearch, pub selection: Option<PdfSelection>, pub pending_copy: bool,
    pub restore: Option<(usize, f32)>,                      // страница и доля из сессии, применяется после Opened
    pub hover_link: Option<(usize, usize)>,
}
pub enum PdfPhase { EngineMissing { error: Option<String> }, EngineStarting, Loading, Ready, Error(String), PasswordRequired }
```

Правило «поля меняются одним методом»: `PdfTabState::apply_event(&mut self, ev)` — единственное место, меняющее `phase`, `pages`, `text`, `search.matches`; `set_viewport(w, h)` — единственное место пересчёта `layout` и клампа скролла.

### 5.3 Ввод

- Клавиши (`pdf_keys.rs`, вызов из `main_keys.rs` рядом с `handle_database_table_key`): PgUp/PgDn ± высота области, Home/End, ↑/↓ ± `round(48·s)`, Ctrl+F — открыть панель поиска (общий путь), Ctrl+C — копировать выделение, Escape — снять выделение, затем закрыть панель поиска (существующая логика).
- Колесо (`wheel.rs`, рядом с веткой markdown read `:1329-1352`): `scroll.scroll_by` с тем же множителем Ctrl.
- Мышь (`input.rs`): down → фиксируем точку; move с зажатой кнопкой и сдвигом ≥ 3 px → выделение; up без сдвига → клик: ссылка под курсором → действие, иначе снять выделение.
- Курсор (`main_frame.rs:721-737`): над ссылкой — рука, над текстом — I-beam, иначе стрелка.
- Кнопка «Загрузить движок» — `UiId::PdfEngineInstall`, обработчик в `ui_handlers/ui_panels.rs` по образцу `MarkdownModeToggle`; «Отмена» — `UiId::PdfEngineCancel`.

### 5.4 Движок на уровне App

`App.pdf_engine: PdfEngineState { NotStarted, Starting, Ready, Missing, Failed(String), Installing }`, `App.pdf_worker: Option<PdfWorkerHandle>`, `App.next_doc_id: u64`.
- Первый `open_pdf_tab`: `library::locate()` → `Some(path)` → `worker::start` → `Starting`; `None` → `Missing`, все PDF-вкладки в `EngineMissing`.
- `EngineReady` → все вкладки в `EngineMissing`/`EngineStarting` получают `Open`. `EngineFailed(msg)` → `Failed(msg)`, вкладки → `EngineMissing { error }` **без кнопки** (повторный bind в процессе невозможен, §4.3): текст «движок PDF остановлен: <msg>. Перезапустите RRiter». Кнопка «Загрузить»/«Повторить» есть только в состоянии `Missing` (библиотека не найдена или загрузка не удалась).
- Активация вкладки: текстуры и `pending_bitmaps` есть только у **активной** PDF-вкладки — при деактивации они освобождаются (`pdf_textures_to_free`), `requested` очищается, `wanted` = пусто; при активации `set_viewport` и обычный цикл запросов (1–3 страницы по ~10 мс). Панель поиска при активации показывает состояние этой вкладки; если панель открыта с непустым запросом — `update_search` перезапускается для неё. Статус-бар берёт «стр. N / M» только у активной вкладки.
- Установка завершилась (§8.3) → `locate()` заново → `worker::start`.
- Закрытие вкладки: хук рядом с `prepare_database_tab_close` (`app_file_tab_methods.rs:217`): `gens.closed = true`, `Close(id)`, текстуры вкладки — в `App.pdf_textures_to_free` (§6.3).

## 6. Раскладка, рендер, память

### 6.1 Раскладка (`PdfLayout`)

Вход: ширина и высота области вкладки в px (тело вкладки: `root_frame_content_frames_renderer.rs:195-240`), масштаб `s`. `margin = round(16·s)`, `gap = round(8·s)`, `page_w = max(64, viewport_w − 2·margin)`; для страницы i: `h_i = round(page_w · h_pt/w_pt)`, `y_i = margin + Σ_{j<i}(h_j + gap)`; `total_h = y_last + h_last + margin`. Все значения целые px. `ScrollState` получает `content = total_h`, `viewport = viewport_h`. Пересчёт только при изменении `viewport_w`/`s` или `pages` (O(N), N ≤ тысячи). Видимый диапазон — бинарный поиск по `y_i` от `scroll.offset` и `offset + viewport_h`. Единый якорь для статус-бара, сессии и `wanted`: `anchor_page` — первая страница с `y_i + h_i > offset` (верхний край области), `anchor_frac = clamp((offset − y_i) / h_i, 0, 1)` (в зазоре перед страницей — 0). Все размеры — физические пиксели (рендерер работает в них, `s` — масштаб DPI).

### 6.2 Рендерер

В `renderer` (файл по месту `draw_icon`, `src/render_view/ui.rs:479-492`, и `renderer_init_methods.rs:505-535`):
- `upload_rgba(&mut self, w: u32, h: u32, rgba: &[u8]) -> glow::Texture` — RGBA8, `LINEAR`, `CLAMP_TO_EDGE`, без mipmap;
- `delete_texture(&mut self, tex: glow::Texture)`;
- `draw_texture_quad(&mut self, tex: &glow::Texture, x, y, w, h: f32)` — вынесено из `draw_icon` (flush → bind → quad mode 1 → flush → rebind атласа); `draw_icon` становится вызовом `draw_texture_quad`. Одна дополнительная пара flush на страницу; видимых страниц 1–3.
- Размер растра ограничен по обеим сторонам: `width_px = min(page_w, 4096, floor(4096 · w_pt / h_pt))`, `height_px = round(width_px · h_pt / w_pt)` — ни одна сторона не больше 4096, битмап ≤ 64 МБ. Рисуется растянутым до `page_w × h_i` (обычно 1:1; на 4K с широкой областью или для очень высоких страниц — с масштабом).

### 6.3 Кэш текстур и загрузка

`PageTexture { tex: glow::Texture, width_px: u32, height_px: u32, gen: u32 }`. Правила:
- нужны страницы `visible ± 2` (`wanted`); для каждой без текстуры текущего `gen` и без запроса в полёте — `Render { width_px, gen }`; `requested: HashSet<(page, gen)>`;
- смена `page_w`/`s` → `gens.render += 1`, старые текстуры **не** удаляются до прихода новых (рисуются растянутыми, без мигания); при замене старая текстура страницы уходит в `pdf_textures_to_free`; `Page` с устаревшим `gen` или для страницы вне `wanted` — отбрасывается до `upload` (буфер освобождается);
- страница вышла за `wanted` → текстура в `App.pdf_textures_to_free`; `pending_bitmaps` для таких страниц выбрасываются;
- `pending_bitmaps` загружаются в начале `draw_root_pdf_frame` (там есть `&mut Renderer`): не больше 2 за кадр, остальные ждут следующего кадра (ещё один redraw запрашивается, пока очередь не пуста); после `upload_rgba` `Vec<u8>` освобождается;
- **единственный владелец освобождения** — `App.pdf_textures_to_free: Vec<glow::Texture>`, куда текстуры складывают замена, выход из `wanted`, деактивация и закрытие вкладки; дренируется в начале каждого кадра (`main_frame.rs` перед отрисовкой корня), поэтому текстуры закрытой или неактивной вкладки удаляются даже если PDF-кадр не рисуется.

Бюджет: текстуры только у активной вкладки (§5.4), не больше `|wanted|` ≤ 7 штук по ≤ 64 МБ каждая, типично A4 при `page_w` 1800 px → 18 МБ × 5 ≈ 92 МБ VRAM; CPU-копии живут только между событием и загрузкой (≤ 7). Воркер: RSS ~40 МБ + один битмап в полёте.

### 6.4 Отрисовка (`pdf_view.rs`)

Порядок в кадре: фон области → для каждой видимой страницы: текстура (или плейсхолдер: прямоугольник «бумага» + «стр. N» по центру) → подсветка совпадений (`push_rect` с альфой, текущий — насыщеннее) → выделение (`push_rect` с альфой) → скроллбар (`draw_scrollbar`) → экраны состояний (EngineMissing/Loading/Error/PasswordRequired — текст по центру области, кнопки через `register_button`). Регистрация в `ui_registry`: `UiId::PdfPage(page)` для видимых страниц (clip областью вкладки), `UiId::PdfLink(page, idx)` для ссылок видимых страниц с загруженным текстом. Все Y — целые (`scroll.offset` округлён, `y_i` целые), текст — через существующие helper'ы статус-бара/диалогов.

Пересчёт pt → px: `k = page_w / w_pt`; `px = (x·k).round()`, аналогично y относительно `y_i − offset`.

Статус-бар: `draw_status_bar(progress_label = "стр. N / M")` для активной PDF-вкладки (там канал `progress_label`, `ide_panel_dialog_renderer.rs:203`).

### 6.5 Тёмная тема (перекраска объектов в воркере)

Факт (проверено по исходникам `pdfium-render` 0.9.4): `PdfRenderConfig` цветовой схемы не имеет, `FPDF_RenderPageBitmapWithColorScheme_Start` есть только в trait биндингов, а `bindings()`, `page_handle()`, `handle()` — `pub(crate)`; без форка сырой вызов недоступен. Поэтому:

- `PdfRequest::Render` получает поле `dark: bool`; `PdfTabState.dark: bool`; переключение → `gens.render += 1` (старые текстуры до прихода новых, §6.3). Настройка одна на приложение (`settings`, ключ `pdf_dark_pages`), по умолчанию `true`, если у редактора активна тёмная тема (наличие признака темы уточняется на плане; нет признака — по умолчанию `false`). Переключатель `UiId::PdfDarkToggle` в статус-баре PDF-вкладки.
- Backend при `dark`: после загрузки страницы обходит её объекты (рекурсивно в form-XObject группы); для path- и text-объектов читает fill/stroke цвет и выставляет цвет с **инвертированной яркостью при сохранении оттенка** (HSL: `L' = 1 − L`, H и S без изменений; альфа как была); image-объекты не трогает; shading не перекрашивается (ограничение pdfium, градиенты остаются светлыми); объекты, у которых `set_*_color` вернул ошибку, пропускаются. Фон битмапа — `set_clear_color(тёмный цвет «бумаги» темы)`. Правки живут в загруженной `PdfPage`; она закрывается после рендера, `GenerateContent` не вызывается — документ не меняется, светлый рендер той же страницы даёт оригинал.
- Стоимость: обход объектов на страницу в воркере (тысячи объектов на векторных страницах — оценка ≤ 20 мс, замер на плане на harness из spike); в кадре — ничего. Текст/ссылки/поиск от темы не зависят.
- Плейсхолдер и экраны состояний в тёмном режиме — цвета темы редактора, как везде.
- Не в v1: инверсия картинок/сканов (негатив), собственная схема из четырёх цветов (`FPDF_COLORSCHEME`) — если понадобится, через PR в `pdfium-render`, открывающий `bindings()`.

## 7. Текст: поиск, выделение, ссылки (`src/app/pdf_tab/text.rs`)

### 7.1 Кэш текста

Для страниц `visible ± 1` без `text[page]` и без `text_requested[page]` — `PageText { page }`; событие `Text` кладёт `Arc<PageText>` и `links`. Кэш живёт до закрытия вкладки (~50 КБ на страницу текста; 500 страниц ≈ 25 МБ — верхняя граница, обычный документ на порядок меньше). Не ограничиваем в v1.

### 7.2 Поиск

`PdfSearch { gen: u32, query: String, matches: Vec<PdfMatch { page, start, end }>, current: Option<usize>, done: bool, pending_jump: Option<(u32, usize)> }` (`pending_jump` — `(gen, idx)`, устаревший `gen` отбрасывается). Enter до `SearchDone` циклит по уже пришедшим совпадениям; новые дописываются в хвост, `current` не сдвигается.
- `update_search` (`app_file_tab_methods.rs:854`): для активной `Pdf`-вкладки — `gens.search += 1`, `matches.clear()`, `current = None`, `done = false`; пустой запрос → без отправки; иначе `Search { query: <тот же паттерн, что строит редакторская ветка>, gen }`.
- `SearchPage` с актуальным `gen` дописывает совпадения (страницы приходят по порядку, `matches` остаётся отсортирован по `(page, start)`); первое пришедшее совпадение становится `current = Some(0)` и вызывает переход. `SearchDone` → `done = true`.
- `jump_to_search_result(next/prev)`: `current` циклически; прокрутка так, чтобы прямоугольник совпадения (объединение rect'ов chars `start..end` по строкам — первая строка) оказался в верхней трети области; если `text[page]` ещё нет — `pending_jump = Some(idx)`, запрос текста, переход при `Text`.
- Панель поиска показывает счётчик «k / n» и, пока `!done`, «n+»: ветка в отрисовке панели (`render_view/search.rs:46`) для `Pdf`.
- Подсветка (§6.4): по видимым страницам — диапазон `matches` для страницы бинарным поиском; прямоугольники строк из rect'ов chars (`line_rects(chars, start, end)` — общий helper с выделением: соседние chars с пересекающимися по Y rect'ами объединяются в один прямоугольник).

### 7.3 Выделение и копирование

`PdfSelection { anchor: (usize, usize), head: (usize, usize) }` — `(page, char_idx)`; упорядоченный диапазон — по `(page, idx)`.
- Hit-test: точка → страница по `layout` → page-local pt → символ: первый `rect` содержащий точку (rect нулевой площади — пропускается); иначе ближайший по строке (минимальная |Δy| до центра rect, затем минимальная |Δx|); страница без загруженного текста → `head` не меняется (текст видимых страниц запрашивается при появлении, §7.1, так что окно в миллисекунды).
- Drag обновляет `head`; курсор у верхнего/нижнего края области (≤ `round(24·s)` px) — автопрокрутка `round(12·s)` px за тик анимации скролла в ту сторону; отрисовка — `line_rects` для каждой видимой страницы в диапазоне (первая/последняя страницы — частично, промежуточные — целиком).
- Ctrl+C: собрать `chars[..].ch` по диапазону в `String` (между страницами — `\n`); если у какой-то страницы диапазона нет текста — запросить и `pending_copy = Some(range)`; копирование выполняется при приходе последнего `Text`, если `selection` всё ещё равен снимку; новый клик/Escape/закрытие — `pending_copy = None`. Буфер обмена — существующий `platform`-путь.
- Ограничение v1: геометрия — прямоугольники символов после переворота Y; повёрнутый/вертикальный текст выделяется и подсвечивается по bbox символов без учёта направления строки (криво, но не падает). Пробелы приходят от pdfium как символы со своим rect и участвуют в тексте копирования.
- Клик без сдвига, Escape, новый документ → `selection = None`.

### 7.4 Ссылки

- Hover/клик: страница → page-local pt → первый `links[page]` с `rect` содержащим точку; `hover_link` — для курсора.
- `Uri(url)` → `platform::open_url(self.external_requests.sink(), url)` (`platform/desktop_request.rs:210`; только http/https, остальное backend уже отфильтровал).
- `Page { page, y_pt }` → `scroll.clamp_target(y_page + round(y_pt·k) − margin)` с анимацией, как у колеса; `y_pt = None` → верх страницы.
- Ссылка «сработает» только по клику без drag (≥ 3 px сдвига — это выделение).

## 8. Поставка библиотеки

### 8.1 `pdfium.json` (корень репозитория)

```json
{ "version": "chromium/8066",
  "platforms": {
    "linux-x64":   { "archive": "pdfium-linux-x64.tgz",   "sha256": "…", "lib": "lib/libpdfium.so" },
    "win-x64":     { "archive": "pdfium-win-x64.tgz",     "sha256": "…", "lib": "bin/pdfium.dll" },
    "mac-arm64":   { "archive": "pdfium-mac-arm64.tgz",   "sha256": "…", "lib": "lib/libpdfium.dylib" },
    "mac-x64":     { "archive": "pdfium-mac-x64.tgz",     "sha256": "…", "lib": "lib/libpdfium.dylib" } } }
```
URL: `https://github.com/bblanchon/pdfium-binaries/releases/download/<version>/<archive>`. Значения `lib` — пути внутри архива (проверить на плане по содержимому архивов; win может быть `bin/pdfium.dll`). Обновление версии = правка этого файла + перепрогон `make pdfium` и headless-тестов PDF.

`src/pdf/library.rs`: `include_str!("../../pdfium.json")`, разбор `serde_json` при старте воркера (не в кадре); ключ платформы из `cfg!(target_os, target_arch)`; неизвестная платформа → `Missing` с сообщением «платформа не поддерживается» и без кнопки. Файл `.json` в корне — сознательное исключение из AGENTS §3 (список расширений для новых файлов), утверждено пользователем 30.09 вместе со спекой (в AGENTS §3 добавить `pdfium.json` как именованное исключение); альтернативы (константы в Rust + дубль в Python, `.txt` с JSON внутри) хуже: два источника истины либо файл, который не подсвечивает ни один редактор.

Почему загрузчик не Rust-CLI режим приложения, а Python: build-скрипты и `make codex_test` должны получить библиотеку **до** того, как есть собранный бинарь (Windows/macOS-пакетирование, тестовый прогон), а Python уже владеет пакетированием. Логика в двух языках — 40 строк (URL, SHA-256, tar), принимается.

### 8.2 Поиск библиотеки (`locate()`)

1) `RRITER_PDFIUM_PATH` задана → **только** она: файл есть — используем, нет — `Missing` с текстом «RRITER_PDFIUM_PATH указывает на несуществующий файл» (без фолбэка, чтобы тесты и отладка были детерминированы). 2) иначе первый существующий: рядом с exe `<exe_dir>/<lib_name>`, на macOS ещё `<exe_dir>/../Frameworks/<lib_name>`; затем `data_dir()/tools/managed/pdfium/<version-slug>/<lib_name>` (`version-slug` = `chromium-8066`). `lib_name` — базовое имя из `lib`. Только проверка существования, без чтения. На macOS release с hardened runtime библиотека из data-dir не загрузится (library validation требует подпись тем же Team ID) — это ожидаемо: release-бандл несёт её в `Frameworks/`, а путь загрузки — для dev-сборок без подписи; если bind всё же упал, экран показывает путь и причину.

### 8.3 Загрузка

Реализуется как новый вид установки в `tool_installer`/`tool_installer_download.rs` (те же `ToolInstallPhase`, `Arc<AtomicBool>` отмены, прогресс каждые 64 КиБ, `async_http_client_builder`, каталоги поколений `data_dir()/tools/managed/<key>/<generation>`), ключ `pdfium`. Отличия: после загрузки — SHA-256 архива (`sha2`) против манифеста, несовпадение → ошибка «хеш не совпал», файл удалён; распаковка `flate2` + `tar`, из архива берётся только `lib`; результат в `…/pdfium/<version-slug>/<lib_name>` через атомарную замену (`platform`). Старые каталоги других версий удаляются после успешной установки. Никаких автоповторов и загрузки на старте; запуск только кнопкой. Нет сети → ошибка в фазе, текст в экране `EngineMissing`, кнопка «Повторить».

### 8.4 `make pdfium`, build-скрипты, тесты

- `scripts/fetch_pdfium.py [--dest DIR] [--platform KEY]`: читает `pdfium.json`, качает, проверяет SHA-256, распаковывает `lib` в `DIR` (по умолчанию — управляемый каталог из §8.2 для текущей платформы). Идемпотентен: файл есть — ничего не делает. Одна реализация логики URL/хеша для Makefile и build-скриптов (Rust-сторона повторяет только `locate()` и загрузку по кнопке).
- `make pdfium` → `python3 scripts/fetch_pdfium.py`. `make codex_test` и `make test` зависят от `pdfium` и экспортируют `RRITER_PDFIUM_PATH=<файл>` тестам (тестовый data-dir — per-PID, §0 AGENTS.md, поэтому управляемый путь тесты не увидят).
- `build_windows.py`/`build_macos.py`: вызывают `fetch_pdfium.py --dest <staging> --platform <key>`, кладут библиотеку рядом с exe / в `Contents/Frameworks/`, macOS — `codesign` библиотеки до подписи бандла, в `--self-test` проверяют наличие файла. Linux-релиза нет (пользователь один; README-задача `lok6ip2guqrhces4r4kkb9mu`).

## 9. Сессия

`state_persistence.rs`: строка `PDF\t` + JSON `{"path": <encode_persisted_path>, "page": N, "frac": f}` — `anchor_page`/`anchor_frac` из §6.1. Чтение: как `API\t`/`DBTABLE\t`; при восстановлении вкладка создаётся с `restore = Some((page, frac))`; применяется, когда выполнены оба условия: пришло `Opened` и был хотя бы один `set_viewport` с ненулевой областью (иначе раскладки нет); `page ≥ page_count` → клампится к последней. Старые сборки прочтут строку как путь файла и молча выкинут — приемлемо (версионирования формата нет, `state_persistence.rs:230-233`). В `cfg(test)` сессия — no-op, как сейчас.

## 10. Headless и тесты

### 10.1 `dump`

К записи вкладки (`headless/dump.rs:295-310`): `kind: "pdf"`, объект `pdf: { phase, page_count, current_page, search_matches, search_done, selection_chars, engine }` (`engine` — `PdfEngineState` строкой). Регистрируемые id `PdfPage(n)`, `PdfLink(page, idx)`, `PdfEngineInstall`, `PdfEngineCancel` попадают в массив `ui` как есть.

### 10.2 Fixture

Rust-helper в `ui_tests_pdf.rs`: `write_fixture_pdf(dir) -> PathBuf` собирает минимальный PDF-1.4 строкой с вычисленным xref: 2 страницы Letter, шрифт Helvetica (base-14, без встраивания), стр. 1 — текст «Hello PDF viewer» и «Go to second», аннотации Link: URI `https://example.com/` над первым словом и GoTo на стр. 2 над вторым; стр. 2 — «Second page target». Смещения xref и `/Length` считаются в байтах (текст ASCII, но helper считает по `len()` байтов, не символов). Для сценария ошибки — `write_garbage(path)`: текстовый файл «this is not a pdf» (гарантированно `FormatError`; обрезанный PDF pdfium может восстановить по xref, поэтому не годится). Бинарные fixture в репозиторий не кладём (AGENTS §3).

### 10.3 Сценарии (все через `rriter_headless`, ожидания — `wait_until`, лимит прогонов агенту — в брифе)

1. Открыть fixture → `phase = Ready`, `page_count = 2`, `PdfPage(0)` в `ui`, `current_page = 0`; снимок.
2. Ctrl+F «Second» → `search_matches = 1`, `search_done`; Enter → `current_page = 1`.
3. Клик по `PdfLink` GoTo → `current_page = 1`; Home → `current_page = 0`.
4. Клик по `PdfLink` URI → внешний запрос зафиксирован headless-стабом (`external_requests`), вкладка не изменилась.
5. Выделение drag по строке «Hello PDF viewer» → `selection_chars > 0`; Ctrl+C → буфер обмена (headless-стаб) содержит «Hello».
6. Не-PDF файл с расширением `.pdf` → `phase = Error`, приложение живо (следующая команда отвечает).
7. `RRITER_PDFIUM_PATH=/nonexistent/libpdfium.so` (переменная выставляется в тестовом процессе до старта App; по §8.2 фолбэка нет) → `phase = EngineMissing`, `PdfEngineInstall` в `ui`; клик по нему в headless **не** качает (в `cfg(test)` установка — no-op с фазой ошибки «отключено в тестах»).
8. Открыть `.pdf` дважды из дерева → одна вкладка.
9. Две PDF-вкладки: переключение туда-обратно → `dump` активной показывает её `current_page`/`search_matches`, а не соседней; закрыть вкладку сразу после открытия (до `Opened`) → приложение живо, `dump` без этой вкладки.
10. Resize окна (`resize` драйвера) при открытой PDF → страницы перерисованы по новой ширине (снимок), `phase = Ready`.

Гонки закрытия/поколений покрываются сценариями 9–10 через реальный воркер; fake-backend не заводим (лишняя абстракция ради тестов — против §1 AGENTS).

Сеть в тестах не используется. Библиотеку тестам даёт `make codex_test` (§8.4).

## 11. Ошибки и краевые случаи

| ситуация | поведение |
|---|---|
| библиотека не найдена | `EngineMissing`, кнопка «Загрузить PDF-движок (3,7 МБ)»; после установки — все PDF-вкладки открываются сами |
| bind упал (несовместимая/битая библиотека) | `EngineMissing { error }`, кнопка «Повторить»; в тексте — путь к файлу |
| воркер запаниковал | `EngineFailed` → `pdf_engine = Failed`, вкладки в `EngineMissing { error: "движок PDF остановлен… перезапустите RRiter" }` без кнопки (повторный bind в процессе невозможен, §4.3); crash нативного кода (segfault) не перехватывается — принимаем |
| resize / смена вкладки во время Search | поиск продолжается (состояние на вкладке), совпадения рисуются по новой раскладке; деактивация не отменяет поиск |
| закрытие вкладки до `Opened` | `closed = true`, `Close(id)`; поздний `Opened`/`Page` для этого `DocId` игнорируется |
| файл не PDF / обрезан | `Error("не удалось открыть: …")` в теле вкладки, вкладка остаётся (можно закрыть) |
| пароль | `PasswordRequired`, текст «Документ защищён паролем», без запроса |
| файл удалён/изменён на диске после открытия | как у текстовых вкладок с удалённым файлом (существующее поведение по watcher'у) — вкладка помечается; перечитывание не делаем |
| страница шире 4096 px | текстура 4096, рисуется растянутой |
| 0 страниц | `Error("в документе нет страниц")` |
| страница без текста (скан) | поиск/выделение не находят ничего; ссылки работают |
| смена DPI/масштаба `s` | `set_viewport` → пересчёт раскладки, новый `gen`, старые текстуры до прихода новых |
| нет сети при загрузке | ошибка в экране, «Повторить»; частичный файл удалён |
| хеш не совпал | ошибка «архив повреждён или версия не совпадает», файл удалён |

## 12. Производительность (hot path)

- В `draw_root_pdf_frame`: нет I/O и `format!` кроме подписи плейсхолдера и статус-бара (по одной строке); загрузка ≤ 2 текстур за кадр; итерации только по видимым страницам; совпадения/ссылки видимой страницы — по срезам, без копий; `line_rects` пишет в переиспользуемый `Vec<PtRect>` вкладки (`clear()`).
- Целые пиксели: `scroll.offset` округлён перед отрисовкой; `y_i`, `h_i`, зазоры и поля — целые; прямоугольники подсветки — округлены.
- Ничего из pdfium не вызывается в UI-потоке; `Vec<u8>` битмапа перемещается без копий из события в `upload_rgba`.
- Воркер не кэширует битмапы; кэш текста — `Arc<PageText>` общий между воркером и вкладкой (без копий при `Text`).

## 13. Вне v1 (кандидаты на карточки после задачи)

Zoom; панель outline (данные — один запрос `Outline` в тот же протокол); запрос пароля; Ctrl+G; инверсия для тёмной темы; картинки и Mermaid в markdown-ридере на тех же примитивах текстур (карточка `upf7kgbdyha0d7ypu6s8l3u5`); README по Linux (`lok6ip2guqrhces4r4kkb9mu`); пустая вкладка при отказе открыть бинарный файл (`app_file_tab_methods.rs:366` + `app_window_external_methods.rs:1144-1151`) — отдельная карточка Bugs.

## 14. Открытые вопросы

Нет. Уточняются на этапе плана (факты, не решения): SHA-256 архивов chromium/8066 по платформам (заполняет `fetch_pdfium.py --print-hashes` при первом запуске, значения коммитятся в `pdfium.json`); точные пути `lib` внутри архивов по платформам; API `ScrollState` (`scroll_by`/`clamp_target`/`update`); поддерживает ли `UiId` варианты с параметрами (иначе — `PdfPage(u16)`-подобный общий вариант с индексом); как headless-стаб фиксирует `external_requests` и буфер обмена.
