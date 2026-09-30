# Feature backlog: паритет с Zed / IDEA / PyCharm

Итог опроса 01.10.2026 по сравнению с Zed 1.22, IDEA и PyCharm. Профили сравнения:
RRiter — gap buffer, immediate-mode OpenGL UI, 19 tree-sitter грамматик, LSP только ruff/ty/dart;
Zed — rope+CRDT, GPUI, 14 встроенных + ~140 языков расширениями, DAP, AI, collab, remote.

Сильные стороны RRiter, которых нет в ядре Zed: API client + API mock, PostgreSQL-клиент с SSH-туннелем,
PDF-просмотр, Markdown Read/Edit, установка uv/ruff/ty из редактора, headless-тесты с реальным рендером.

## Принципы (зафиксировано владельцем)

- **Без механизма плагинов.** Не засорять код абстракцией расширений: новый язык/тему/команду добавляем
  сами по просьбе. Языки — встроенными грамматиками.
- **Без Vim mode.** Модальное редактирование не нужно; кастомные хоткеи — да.
- **Без AI внутри редактора** (ghost text, agent panel, inline assistant): агенты живут в терминале.
  Редактор даёт им контекст через свой MCP-сервер.
- **Без collaboration.**
- **Sticky lines вместо breadcrumbs.**

## Добавить

### Навигация
- **Search Everywhere** (единое поле как в IDEA, вкладки-фильтры сверху): команды с показом хоткея,
  файлы (fuzzy, недавние сверху), символы проекта (Go to Symbol через LSP workspaceSymbol / tree-sitter),
  настройки config.json с прыжком в settings-вкладку.
- **Recent files / Switcher** — отдельный попап (Ctrl+E / Ctrl+Tab).
- **Outline / Structure** — панель-дерево классов/функций текущего файла (нужны tree-sitter
  outline-queries или LSP documentSymbol) + Go to Symbol по проекту в палитре.

### Редактирование
- **Snippets / Live templates** — свои шаблоны с плейсхолдерами и Tab-переходом + LSP snippet syntax
  в completion.
- **Soft wrap** — только для markdown/текста, код без переноса. Поле `is_soft_wrap` уже есть, всегда false.
- **Multibuffer** (Zed) — результаты поиска/diagnostics/references как один редактируемый буфер
  из фрагментов разных файлов.
- **Spell check** — комментарии/строки/markdown, ru+en (hunspell-словари).
- **Code inspections широко** — свой движок tree-sitter-инспекций для языков без LSP
  (незакрытые скобки, trailing whitespace, дубли ключей JSON/TOML, unused и т.п.).

### LSP
- **Rename по проекту + Find Usages** — ответы уже приходят и выбрасываются:
  `src/app/events/about_tick_lsp_sections.rs:139-142` (`ReferencesResponse`, `PrepareRenameResponse`,
  `RenameResponse`, `FormattingResponse` → `{}`). Самый дешёвый выигрыш по паритету.
- **Format document + format on save + organize imports** (ruff format, dart format).
- **Произвольные LSP-серверы из конфига** — секция в config.json: команда, языки, initializationOptions
  (rust-analyzer, gopls, typescript-language-server…).
- **Рефакторинги через LSP** — показывать все code actions сервера (extract/inline/move где даёт сервер),
  своих не писать.

### Запуск, отладка, тесты
- **Debugger (DAP)** — брейкпоинты в gutter, step/continue, variables/call stack/watch; debugpy, Dart DAP,
  codelldb.
- **Run configurations / Tasks** — сохранённые команды, кнопка Run, «запустить этот файл», runnables
  в gutter, вывод в панель.
- **Test runner** — панель тестов (pytest / cargo test), запуск одного теста из gutter, дерево
  зелёный/красный, перезапуск упавших.
- **Docker / compose** — панель контейнеров: список, логи, exec, up/down.

### Git
- **Blame** — inline под курсором + колонка в gutter.
- **Ветки и stash в UI** — переключение/создание/удаление веток, checkout из графа, stash push/pop/list.
- **Merge conflict resolver** — 3-панельный (ours / result / theirs) как в IDEA.
- **GitHub/GitLab** — список и создание PR, ревью-комментарии в редакторе, статус CI у коммита (gh API).

### Инструменты и панели
- **Project search: Replace** с preview и выбором совпадений, фильтры include/exclude glob,
  область «открытые файлы / папка».
- **Image viewer** (PNG/JPG/SVG/WebP, zoom, размеры; image/resvg уже в зависимостях) + **hex viewer**.
- **Compare files / Diff viewer** — два файла, файл с буфером обмена, две папки; side-by-side
  с редактированием (сейчас diff только в git-вкладке).
- **Встроенный Kanban (аналог Kanri)** — доски/колонки/drag&drop, карточки markdown + теги;
  связь карточка ↔ `file:line` в обе стороны; CLI/MCP, чтобы агенты заводили и двигали карточки
  как сейчас `kanri-add`. Импорт из Kanri — не нужен.
- **Local history** — вводные для brainstorming: снимки всего проекта (не пофайлово), таймлайн + diff
  в UI, мало места и быстрый доступ; кандидаты — btrfs snapshot или git-объекты в отдельной БД
  (обсудить, не решено).
- **MCP-сервер rriter** — внешний агент (Claude Code в терминале) видит открытые файлы, выделение,
  diagnostics; может открыть файл/diff в IDE. Аналог IDE-плагинов Claude Code.

### Настройки
- **Keymap** — keymap.json + редактор в settings, переназначение любого действия, пресеты.
- **Темы** — несколько встроенных + свой JSON-формат. Импорт VS Code/Zed не нужен.

### Языки
- **Много грамматик**: сначала конфиги (YAML, Dockerfile, .env, nginx, INI, XML), затем всё,
  что есть в tree-sitter. Следить за ростом бинарника и старта.

### Платформы и remote
- **Remote development по SSH** — агент `rriter --server` на хосте (дерево, правка, терминал, LSP
  на удалённой стороне), как в Zed.
- **CI-сборки Windows и macOS** — репозиторий публичный, GitHub Actions бесплатны без лимита минут
  на `ubuntu-latest` / `windows-latest` / `macos-latest`; скрипты `scripts/build_windows.py`
  и `scripts/build_macos.py` уже есть, CI нет.

## Не добавлять (с причиной)

| Фича | Причина |
|---|---|
| Vim / Helix mode | Не пользуюсь; хоткеев достаточно |
| Breadcrumbs | Sticky lines дают тот же контекст |
| Ghost text / edit prediction | Не пользуюсь, пишу через агентов |
| Agent panel (ACP или свой) | Терминала с Claude Code хватает |
| Inline assistant | Не пользуюсь |
| Плагины / WASM API | Не хочу абстракцию для плагинов в коде; нужное добавляю сам |
| Импорт тем VS Code/Zed | Достаточно своих |
| Jupyter / REPL / .ipynb | Не делаю data science |
| Collaboration | Работаю один |
| Welcome / recent projects / несколько окон | Multi-folder workspace уже есть, остальное не нужно |
| Импорт из Kanri | Не нужен |
| TODO/FIXME-панель | Вместо неё встроенный Kanban |
