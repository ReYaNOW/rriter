# Инвентаризация горячих клавиш

Снимок worktree rriter-hotkeys, 2026-10-03; пути от src/app, если не указано иное. `ctrl`/primary в коде = platform::primary_shortcut_modifier (main_keys.rs:177; editor_keys.rs:435): Cmd на macOS, Ctrl иначе; Ctrl+Alt на Windows отсекается как AltGr (src/platform.rs:341-352). `if ctrl` не отвергает лишние модификаторы. mod — платформенный primary (spec §2.2).

## 1. Команды

Guard приведён из текущей ветки; лакс = дополнительные модификаторы не отсекаются. Каждый guard/key проверен по указанному источнику.

| Source | Кодовая проверка | Контекст / действие | Предложение: id — label — KeyContext — default |
|---|---|---|---|
| main_keys.rs:198-219 | show_settings && tool_installer.is_log_open(); KeyC if ctrl | Лог установщика открыт; копировать лог. | settings.copy_installer_log — «Копировать журнал установки» — Settings — mod+c |
| main_keys.rs:120-150,225-240,529-534 | отвергает shortcut если !primary || !shift || physical_key != KeyV | Markdown документ, не выше владелец ввода; toggle чтения/редактирования; repeat потребляется. Источник — KeyV, не KeyM как в recon. FAQ подтверждает Ctrl/Cmd+Shift+V (app_bootstrap.rs:34). | markdown.toggle_mode — «Markdown: режим чтения/редактирования» — Markdown — mod+shift+v |
| main_keys.rs:265-285 | Pressed && alt && physical_key == KeyQ; helper получает shift_key() (:271-274) | IDE only (`if self.is_ide_mode`, :269; вне IDE ветка не возвращает и клавиша идёт дальше). Alt+Q: панель закрыта → открыть (+spawn), открыта → toggle фокуса (:26-35). Shift+Alt+Q: открыта → закрыть, закрыта → открыть (:10-25). Лакс: Ctrl+Alt+Q тоже. | terminal.toggle_focus — «Показать/скрыть терминал» — Global — alt+q; terminal.close — «Закрыть терминал» — Global (срабатывает и без фокуса терминала) — alt+shift+q |
| main_keys.rs:662-677 | если Settings открыты: KeyF1; дальше KeyF1 && !term_focused | F1 закрыть или открыть Settings; без modifier check. В фокусе терминала F1 уходит в терминал как `\x1bOP` (keyboard.rs:852 → terminal_key_sequence). | settings.toggle — «Открыть/закрыть настройки» — Global, кроме фокуса терминала — f1 |
| main_keys.rs:670-684 | KeyF8, если !term_focused | Toggle FPS; FAQ F8 (app_bootstrap.rs:41). В фокусе терминала — в терминал. | view.toggle_fps — «Показать/скрыть FPS» — Global, кроме фокуса терминала — f8 |
| main_keys.rs:687-695 | is_ide_mode && alt && KeyW | Toggle Problems; Ctrl+Alt+W также проходит, нестрогий вариант не документирован (FAQ app_bootstrap.rs:39-41). | view.toggle_problems — «Показать/скрыть проблемы» — Global — alt+w |
| main_keys.rs:698-709 | is_ide_mode && ctrl && shift_key() && KeyF | IDE: project search до дальнейшей маршрутизации. | search.project.open — «Поиск по проекту» — Global — mod+shift+f |
| main_keys.rs:721; file_tree_dialog.rs:440-455 | ctrl && KeyZ | File tree focused, Settings закрыты: undo операции. Helper проверяет !file_tree_focused || show_settings. | file_tree.undo — «Отменить операцию с файлами» — FileTree — mod+z |
| main_keys.rs:721; file_tree_dialog.rs:457-507 | selection nonempty; !ctrl возвращает false; match KeyC/KeyX/KeyV | File tree focused: copy/cut/paste selection. | file_tree.copy/cut/paste — «Копировать/вырезать/вставить файлы» — FileTree — mod+c/x/v |
| main_keys.rs:826-829; app_ide_tab_methods.rs:716-721 | ctrl && Pressed && switch_tab_from_keyboard(physical_key); helper matches PageDown => next, PageUp => previous | IDE tab strip cycles to next/previous tab; returns false for other keys and outside IDE (вне IDE Ctrl+PgUp/PgDn падает в редакторный PageUp/PageDown, editor_keys.rs:925-938). Стоит до финальной маршрутизации (main_keys.rs:826 < :841), поэтому работает и в фокусе терминала/поиска — контекст не Editor. | tabs.switch_next — «Следующая вкладка» — Global (IDE) — mod+pagedown; tabs.switch_previous — «Предыдущая вкладка» — Global (IDE) — mod+pageup |
| main_keys.rs:57-66,832-839 | primary && Digit4 && terminal open && (terminal_focused || term_search_focused) | Close active terminal tab. | terminal.close_tab — «Закрыть вкладку терминала» — Terminal — mod+4 |
| main_keys.rs:732-766 | ctrl && KeyC; сначала graph selection, далее git_logs_keyboard_copy_eligible(...) | IDE: копировать выделение Git graph/log, иначе continue dispatch. | git.copy_selection — «Копировать выделение Git» — Global — mod+c |
| main_keys.rs:383-393 | Enter или NumpadEnter if ctrl | Активный SQL query: запуск. | database.query.run — «Выполнить SQL-запрос» — Database — mod+enter, mod+numpadenter |
| main_keys.rs:394-400 | Space if ctrl | SQL completion. | database.query.complete — «Дополнение SQL» — Database — mod+space |
| editor_keys.rs:440-447 | Pressed && ctrl && shift && KeyO && active_document_is_markdown() | Toggle Markdown TOC. | markdown.toggle_toc — «Оглавление Markdown» — Markdown — mod+shift+o |
| editor_keys.rs:450-454,786-792 | welcome: KeyO if ctrl; editor: KeyO if ctrl | Open picker; dirty document asks confirmation; PDF filter can block. | file.open — «Открыть файл» — Editor (проверяется и на Welcome-сайте :452, т.е. активна в Editor и Welcome) — mod+o |
| editor_keys.rs:455-483 | show_welcome: KeyQ if ctrl | Save window config and quit. Достижимо только на экране приветствия: welcome-ветка возвращает при любой клавише (:486) до центрального match. | app.quit — «Выйти из RRiter» — Welcome — mod+q |
| editor_keys.rs:719-734 | KeyQ if ctrl | !show_welcome: IDE → CloseAllTabs (:720-725), вне IDE → CloseFile (:727-731); dirty → подтверждение. FAQ «Ctrl + Q Выйти из редактора (закрыть документ)» (app_bootstrap.rs:17) описывает обе ветки. | Решено: вне IDE один документ, «закрыть все» = «закрыть текущий» → одна команда tabs.close_all — «Закрыть все вкладки / документ» — Editor — mod+q (ветвление по is_ide_mode в теле; это CloseAllTabs фильтра PDF, spec §3.6). app.quit — отдельно, Welcome (§6). |
| editor_keys.rs:735-739 | KeyF1 | Вторая проверка settings.toggle (до неё обычно доходит только F1 из PDF-фильтра). | settings.toggle (тот же id) |
| editor_keys.rs:740-750 | KeyF if ctrl | Find in editor; IDE Ctrl+Shift+F consumed earlier as project search. | search.editor.open — «Найти в файле» — Editor — mod+f |
| editor_keys.rs:751-766 | KeyW if ctrl | Expand syntax selection. | editor.expand_selection — «Расширить выделение» — Editor — mod+w |
| editor_keys.rs:777-785 | KeyS if ctrl | Save; hidden editor guard blocks under PDF/image. FAQ Ctrl+S (app_bootstrap.rs:15). | file.save — «Сохранить файл» — Editor — mod+s |
| editor_keys.rs:793-821 | KeyZ if ctrl | Undo. FAQ Ctrl+Z (app_bootstrap.rs:29). | editor.undo — «Отменить» — Editor — mod+z |
| editor_keys.rs:822-850 | KeyY if ctrl | Redo. FAQ Ctrl+Y (app_bootstrap.rs:30). Решено: ветки Ctrl+Shift+Z в редакторе нет; он попадает в лаксовую `KeyZ if ctrl` (:793) = **undo**. Redo по Ctrl+Shift+Z — только в полях (single_line_input.rs:264-267, тест database_table_cell_edit_state.rs:380; API api_client_app_request_methods.rs:561-565). | editor.redo — «Повторить» — Editor — mod+y. mod+shift+z — только как осознанное изменение поведения редактора (spec §3.2 ошибается). |
| editor_keys.rs:1057-1069 | KeySlash if ctrl; comment marker called with physical_key and ctrl | Editable language with marker: toggle line comment. | editor.toggle_line_comment — «Закомментировать строку» — Editor — mod+/ (токен клавиши `/`, не `slash`: key_input.rs:103) |
| editor_keys.rs:1070-1079 | KeySpace if ctrl | Editor completion. | editor.complete — «Автодополнение» — Editor — mod+space |
| editor_keys.rs:1100-1103 | Digit4 if ctrl | Close editor tab. | tabs.close — «Закрыть вкладку» — Editor — mod+4 |
| editor_keys.rs:1104-1139 | KeyC if ctrl | Copy autocomplete/hover/graph selection if present, else editor selection. | edit.copy — «Копировать» — Editor — mod+c |
| editor_keys.rs:1140-1149 | KeyX if ctrl | Cut editor selection. | edit.cut — «Вырезать» — Editor — mod+x |
| editor_keys.rs:1150-1174 | KeyV if ctrl | Paste clipboard, including multicursor. | edit.paste — «Вставить» — Editor — mod+v |
| editor_keys.rs:1175-1178 | KeyA if ctrl | Select all. FAQ Ctrl+A (app_bootstrap.rs:35). | edit.select_all — «Выделить всё» — Editor — mod+a |
| pdf_tab/input.rs:5-20 | primary && !alt_key() && KeyC; active PDF and editor input focus | On Pressed copy selection; consumes event. | pdf.copy — «Копировать текст PDF» — Pdf — mod+c |
| file_tree_dialog.rs:457-480 | nonempty selection; Delete/F2 physical key, no modifier test | Delete opens confirmation; F2 renames only one selected path. | file_tree.rename — «Переименовать файл» — FileTree — f2. Delete: голый Delete — Reserved (§2), умолчанием быть не может (spec §2.3) → фиксированная локальная ветка FileTree. |
| image_tab.rs:184-214 | reject Control/Alt/Super/Shift and F1-F24, then Pressed Digit0 | Image tab resets fit (only with editor_has_input_focus, :186). | Решено: не команда. Голая цифра — NeedsModifier (spec §2.3), модифицированный вариант сменил бы умолчание → фиксированная локальная ветка Image (как навигация PDF, pdf_tab/input.rs:35-40); в RESERVED не нужна. |
| database_table_edit_methods.rs:1178-1189 | KeyC/KeyZ if primary && focus.is_none(); Delete/Insert if focus.is_none() | Active DB table (no table_modal), no cell/filter focus. Фокус терминала не проверяется (:931-935, :1083-1085) — см. §4. | database.table.copy — «Копировать строки» — Database — mod+c; database.table.undo — «Отменить изменение таблицы» — Database — mod+z; database.table.add_row — «Добавить строку» — Database — insert (spec §2.3 допускает). Delete — Reserved → фиксированная ветка. |
| database_table_edit_methods.rs:1139-1147 | filter_focus && primary && Space | Фокус в WHERE/ORDER BY: SQL-дополнение фильтра. | database.table.filter_complete — «Дополнение фильтра таблицы» — Database — mod+space |
| editor_keys.rs:626-632 | alt && Enter (лакс: Ctrl+Alt/Shift+Alt+Enter тоже) | Editor: меню быстрых действий LSP. В Markdown read — readonly notice (editor_keys.rs:181). | lsp.code_actions — «Быстрые действия LSP» — Editor — alt+enter |
| editor_keys.rs:186-199 | markdown read mode: KeyC if primary → CopySelection; ArrowLeft/Right, KeyA/KeyW if primary → Consume | Markdown read: копирование выделения рендера; select-all/expand глушатся. | edit.copy (тот же id); Consume проверяет edit.select_all/editor.expand_selection |
| keyboard.rs:713-715 | editor search focused: KeyF if ctrl | Повторный Ctrl+F в поле поиска выделяет запрос. | search.editor.open (тот же id, другое состояние) |
| project_search_input_state.rs:27-31; keyboard.rs:834-891 | project search field focused: Enter/NumpadEnter if ctrl | Запуск поиска по проекту (кроме поля Filter). Подпись ide_panel_project_search_renderer.rs:519. | search.project.run — «Запустить поиск по проекту» — ProjectSearch (фокус в поле поиска, main_keys.rs:711-716) — mod+enter, mod+numpadenter |
| api_client_app_request_methods.rs:428-440 | api_mock_alt_enter_route_target(mock python focus, alt, Enter/NumpadEnter) | Фокус в Python-редакторе API Mock: запуск инструментов маршрута. | api.mock.run_route_tools — «API Mock: инструменты маршрута» — ApiClient — alt+enter |
| api_client_app_request_methods.rs:386-397 | Pressed && ctrl && KeyC && active_tab_is_api_client() && (route text or hover selection copied) | Копирование выделения маршрута/hover на вкладке API. | edit.copy в ApiClient — см. «Клипборд полей» в §2 |
| api_client_app_request_methods.rs:399-415 | Pressed && ctrl && Digit4 && active_tab_is_api_client(); no-focus F2 returns false, otherwise API tab returns true | Close API tab; API Client absorbs its tab's other key input. | tabs.close (тот же id, второй сайт; Editor-маршрут сюда не доходит — API tab поглощает, :415) |

Лаксовые проверки становятся точными; в умолчания — только варианты из FAQ (app_bootstrap.rs:15-41) или тестов (spec §3.3). Ни один лаксовый вариант выше (Ctrl+Alt+Q/W, Shift+F1, Ctrl+Shift+S …) в FAQ нет; тесты на них не проверялись (вывод для Task 2). Save As нет.

## 2. Фиксированные ветки

RESERVED must mirror exact variants accepted after fixed branches become strict, not infer from key names (spec §3.4: docs/superpowers/specs/2026-10-03-hotkeys-design.md:110-114).

| Keys / site | Verified source fact | Reservation note |
|---|---|---|
| ArrowLeft/Right | editor_keys.rs:851-882: `word` → move_word_*(shift), иначе move_*(shift). word = Ctrl (Linux/Windows, на Windows без Alt) / Alt (macOS) (platform.rs:355-360). Те же варианты в полях (single_line_input.rs:330-345, main_keys.rs:611-628). | RESERVED: bare, shift, word, word+shift. |
| ArrowUp/Down | editor_keys.rs:883-904: move_up/down(shift), модификатор кроме Shift не смотрится. | RESERVED: bare, shift. |
| Home/End | editor_keys.rs:905-924: `ctrl` (primary) → начало/конец файла(shift), иначе строки(shift); DB cell/filter то же (database_table_cell_edit_state.rs:281-282). FAQ Ctrl+Home/End (app_bootstrap.rs:24-25). | RESERVED: bare, shift, mod, mod+shift. |
| PageUp/PageDown | editor_keys.rs:925-938: page(shift). mod+PgUp/PgDn — команды tabs.switch_* (§1). | RESERVED: bare, shift. |
| Backspace/Delete | editor_keys.rs:939-1022: `word` → удалить слово, иначе символ; Shift не смотрится. FAQ Ctrl+Bksp/Del (app_bootstrap.rs:36-37). | RESERVED: bare, word. |
| Enter/NumpadEnter | editor_keys.rs:1023-1046 (лакс; Shift+Enter вставляет перевод строки); Shift+Enter = предыдущее совпадение в поиске (keyboard.rs:585-597, 723-737) и перевод строки в body API (api_client_app_request_methods.rs:511). Ctrl+Enter, Alt+Enter — команды (§1). | RESERVED: bare, shift (обе клавиши Enter). |
| Tab | editor_keys.rs:1047-1056; autocomplete Apply (autocomplete_popup_apply_methods.rs:756-760); Shift+Tab — поле назад в диалоге БД (database_app_methods.rs:180-182). | RESERVED: bare, shift. |
| Space | editor_keys.rs:1081-1099 лакс; multicursor допускает text_input_modifiers_allowed (:509-510). Ctrl+Space — editor.complete (:1070-1079). | RESERVED: bare, shift. Alt+Space не резервируется. |
| Escape | Editor Escape at editor_keys.rs:767-775; terminal protocol Escape at keyboard.rs:80; settings close at main_keys.rs:654-668 | Bare Escape reserved; no Ctrl/Alt reservation absent explicit test. |
| Popup navigation | TOC Esc/Up/Down/Enter (markdown.rs:46-69); LSP menu (editor_keys.rs:634-667); autocomplete Esc/arrows/Enter/Tab (autocomplete_popup_apply_methods.rs:743-760); SQL history Up/Down/Home/End/Enter (main_keys.rs:308-352) | Fixed, bare. |
| Terminal protocol | terminal_key_sequence maps Enter/backspace/tab/escape/insert/delete/pages/home/end/arrows/F1-F12 (keyboard.rs:62-114) and control-byte chords (:115-146) | Fixed terminal input, not app commands. |
| Widget editing | Settings ignore field handles Ctrl+A/C/X/V, edit/navigation keys (main_keys.rs:536-645); DB table cell handler includes Enter/Escape and editing (database_table_edit_methods.rs:1162-1200) | Preserve widget ownership and exact local guards. |
| Клипборд полей (mod+A/C/X/V, mod+Z, mod+Y, mod+shift+Z) | single_line_input.rs:264-311 (общий для полей БД/диалогов); editor search keyboard.rs:780-800; terminal search :643-663; LSP filter :924-944; git message :1019-1040; project search project_search_input_state.rs:72-99; API fields api_client_app_request_methods.rs:518-575; settings ignore main_keys.rs:564-592; DDL hover main_keys.rs:455-476; LSP logs :787-797; DB unavailable text database_table_edit_methods.rs:1101-1113. | Не RESERVED (это умолчания edit.*/editor.undo/redo). Ветки полей проверяют те же команды через hit; иначе переназначенный copy не работает в полях. Оставить жёсткими — только как явное исключение в спеке. |

Bare Escape is Reserved and cancels recording; Ctrl+Escape is not reserved solely due to a lax source branch (spec:58-62,207). Ctrl+Enter is a command, not Reserved (main_keys.rs:383-393).

## 3. Фильтры по клавишам

Позиция и защищаемый инвариант — §11. Здесь — во что переводится перечисление клавиш.

| Filter (source) | KeyCode-перечисление сегодня | Набор команд после перевода |
|---|---|---|
| pdf_tab_key_reaches_editor (editor_keys.rs:86-91, вызов :492-496) | F1, Escape; primary+Q/F/O/4 | {settings.toggle, tabs.close_all, search.editor.open, file.open, tabs.close} и ни одной другой команды Editor; + голый Escape |
| Git diff readonly (editor_keys.rs:680-708) | Enter/NumpadEnter/Tab/Space/Backspace/Delete; ctrl+X/V; comment marker | фикс. клавиши + {edit.cut, edit.paste, editor.toggle_line_comment} |
| Markdown read (editor_keys.rs:152-205) | как git diff + ctrl+Z/Y, Alt+Enter → notice; ctrl+C copy; ctrl+A/W, Left/Right consume | notice: {edit.cut, edit.paste, editor.undo, editor.redo, lsp.code_actions}; copy: edit.copy; consume: {edit.select_all, editor.expand_selection} |
| Multicursor (editor_keys.rs:506-521) | text, Space, plain edits, ctrl+V/Z/Y, навигация | {edit.paste, editor.undo, editor.redo} + фикс. клавиши |
| Startup gate (app_ide_startup_methods.rs:705-744) | F1, alt+Q/W пропускаются; primary+S/Z/Y/Q/O/4/Tab/PgUp/PgDn блокируются | пропуск: {settings.toggle, terminal.toggle_focus, terminal.close, view.toggle_problems}; блок: {file.save, editor.undo/redo, tabs.close_all, app.quit, file.open, tabs.close, tabs.switch_*}. primary+Tab — ветки нет (grep `KeyCode::Tab`). |
| PDF input (pdf_tab/input.rs:11-23) | Ctrl/Alt/Super пропускаются кроме primary+C; F1/F8 пропускаются | pdf.copy локально; пропуск: любая команда, {settings.toggle, view.toggle_fps} |
| Image input (image_tab.rs:184-214) | любой модификатор и F1-F24 пропускаются | пропуск всех команд; Digit0 — фикс. (§1) |
| Git logs copy (main_keys_vcs_copy.rs:7-29) | нет KeyCode, только владельцы фокуса | git.copy_selection |

PDF is the safety allowlist; editing classifiers and field filters preserve edit semantics. Spec explicitly requires remapped Save blocked and Find allowed on PDF (design spec:122-124).

## 4. Перехваты терминала

At terminal handler entry primary/terminal Ctrl/terminal Alt are computed (src/app/keyboard.rs:486-489).

| Chord | Exact source | terminal_intercepts |
|---|---|---|
| primary+F | Pressed && primary && KeyF opens search (keyboard.rs:491-504) | true |
| primary+V | primary && KeyV creates paste payload (keyboard.rs:506-522) | true |
| primary+C / Ctrl+C | KeyC if primary || terminal_ctrl; primary+selection copies, terminal_ctrl sends 0x03 (keyboard.rs:527-538) | true |
| Ctrl+A-Z | KeyA..KeyZ if ctrl mapped to control bytes (keyboard.rs:123-146); C/F/V have routes above | true |
| Ctrl+Space, Ctrl+2/6, Ctrl+Minus/Slash/brackets/backslash | explicit if ctrl arms (keyboard.rs:115-122) | false: не входят в список spec §3.5; включение изменило бы умолчание database.query.complete (см. ниже) |
| Ctrl+4 | earlier helper checks primary+Digit4 and terminal focus/search focus (main_keys.rs:57-66,832-839) | app close-tab branch, not byte gateway |

Control bytes use terminal_ctrl and are lax: Ctrl+Shift+letter и Ctrl+Alt+letter (ESC-префикс, keyboard.rs:175-183) тоже дают байт — terminal_intercepts должен включать эти варианты, иначе они пойдут в команды.

Умолчания, которые сегодня срабатывают в фокусе терминала раньше финального маршрута (main_keys.rs:841-855) — исключения из шлюза или проверка порядка:

| Default | Source | Пересекается с terminal_intercepts |
|---|---|---|
| alt+q, alt+shift+q, alt+w | main_keys.rs:265-285, 687-695 | нет (Alt без Ctrl) |
| mod+shift+f | main_keys.rs:698-709 | да на Linux/Windows (Ctrl+Shift+F = байт 0x06 при лаксе) → исключение |
| mod+pageup/pagedown | main_keys.rs:826-829 | нет |
| mod+4 | main_keys.rs:832-839 | нет |
| mod+c (Git graph tooltip selection) | main_keys.rs:732-746: graph copy не проверяет фокус терминала (logs copy проверяет: main_keys_vcs_copy.rs:19-26) | да → исключение или баг |
| mod+c, mod+z (database.table.copy/undo) | main_keys.rs:404 → database_table_edit_methods.rs:1178-1183, без проверки фокуса | да → исключение или баг |
| mod+enter, mod+space, Escape (SQL-консоль) | main_keys.rs:303-402: active_tab_is_database_query без проверки фокуса | только если Ctrl+Space в intercepts |
| mod+z/c/x/v (FileTree) | main_keys.rs:721; file_tree_focused не сбрасывается при фокусе терминала (mouse_ui_element_press.rs:218-222) — вывод, не проверено рантаймом | да → исключение или баг |
| f1, f8 | main_keys.rs:670-685 исключают term_focused | нет; в фокусе терминала уходят в терминал |

Шлюз в начале handle_main_keyboard_input_inner обошёл бы поглощающие модалки (§5: TOC, лог установщика, file-tree/API/Git-диалоги, DB review, confirm_dialog) — сегодня они забирают клавишу и при фокусе терминала (main_keys.rs:180-300). Шлюз ставить после них или включить их в предикат. Do not let new global binding steal terminal-owned chord (spec §3.5: design spec:116-118).

## 5. Поглощающие обработчики

| Entry | Exact catch-all | run_bound_commands placement |
|---|---|---|
| API Client | if handle_api_client_keyboard_input(&key_event) { return; } (main_keys.rs:775-777). With no focused form, F2 returns false but other keys return active_tab_is_api_client (api_client_app_request_methods.rs:408-415). | Before catch-all, after form field routing. |
| Markdown TOC | if markdown_toc.open { handle_markdown_toc_key(...); return; } (main_keys.rs:180-183); хендлер обрабатывает только Escape/Up/Down/Enter и возвращает true для всего остального (markdown.rs:39-75). | Решено: модальный попап, сегодня глушит все сочетания (F1, Alt+Q, Ctrl+Shift+O тоже). run_bound_commands не вызывается; шлюз терминала — после. Закрытие только Escape/кликом. |
| Installer log | show_settings && tool_installer.is_log_open() handles Escape и KeyC if ctrl, затем return (main_keys.rs:198-219). | Модальный: только settings.copy_installer_log; прочие команды не выполняются. |
| File tree / API / Git modal | handle_file_tree_modal_keyboard (file_tree_dialog.rs:182-430) возвращает true для всех клавиш при: API mock field delete (:183), mock route reset (:210), Git confirm (:232), file-tree menu (:254), move (:267), delete (:289), rename (:311), create (:375); вызов main_keys.rs:238-242. | Модалка владеет; команды не выполняются. Исключение: mod+shift+v Markdown при rename/create (main_keys.rs:238-239). |
| Database review | When review open Escape rollback/Enter commit then unconditional return (main_keys.rs:244-262). | Keep confirmation local. |
| Confirm dialog | modal_dialog_open() = confirm_dialog.is_open() (app_window_external_methods.rs:145-147): Escape отменяет, остальное фокусирует диалог и return (main_keys.rs:289-301). Стоит после Alt+Q (:265). | Команды не выполняются. |
| DB prompts / DDL hover / help | database dialog (main_keys.rs:410-415, database_app_methods.rs:145-190); delete/host-key prompt — return на любую клавишу (main_keys.rs:416-429); DDL hover — Escape, Ctrl+A/C, затем return (:448-483); project-search help — return (:485-495). | Владеют вводом; команды не выполняются. |
| DB table modal | handle_database_table_key при table_modal возвращает true (database_table_edit_methods.rs:955-1080). | Владеет вводом. |

run_bound_commands нужен только на входе API Client (spec §4.1); прочие поглощающие обработчики команды не выполняют и сегодня.

## 6. Взаимоисключающие контексты

| Pair | Decision | Source / uncertainty |
|---|---|---|
| Pdf / Image | Exclusive | Single active-tab kind guard (editor_keys.rs:492-495); input handlers check active tab (pdf_tab/input.rs:7; image_tab.rs:218). |
| Global / each local | Compatible | Global routes occur before final focus route (main_keys.rs:698-709,841-854). |
| Editor / Database | Compatible | DB key path then later editor route (main_keys.rs:404-405,854). |
| Editor / ApiClient | Compatible | API handler can return false (main_keys.rs:775-777; api_client_app_request_methods.rs:420-425). |
| FileTree / Editor | Compatible | File tree helper may fall through, editor route follows (main_keys.rs:719-722,854). |
| Terminal / Editor | Exclusive | Финальный маршрут — if/else: терминал (main_keys.rs:848-852) или редактор (:853-854); editor_has_input_focus ложен при terminal_focused/term_search_focused (app_file_tab_methods.rs:622-623). |
| Terminal / Database, FileTree, ApiClient | Compatible | DB-хендлеры (main_keys.rs:303-404) и file-tree (:721) не проверяют фокус терминала (§4); API Client — панель может быть открыта при фокусе терминала. |
| Welcome / Editor | Exclusive | show_welcome: welcome-match и return до центрального match (editor_keys.rs:450-487); editor_has_input_focus требует !show_welcome (app_file_tab_methods.rs:616). Нужен для app.quit vs tabs.close_all на mod+q. |
| Pdf / Database, Image / Database | Exclusive | Один active_tab с одним kind: PDF/Image (editor_keys.rs:492), DB table/query (database_table_app_methods.rs:218-223; database_app_methods.rs:229-233). Только если Database = «активна вкладка БД» (spec §4.1), без панельных команд. database.refresh_selected (панель) ломает это — тогда Compatible. |
| Markdown / Pdf, Image, Database | Exclusive | active_document_is_markdown требует EditorTabKind::Normal в IDE (markdown.rs:629-636). |

Unlisted context pairs default to compatible; exclusive pairs require evidence (spec risk R6: design spec:222).

## 7. Подписи с сочетаниями в UI

| Source | Text |
|---|---|
| app_bootstrap.rs:15-17 | FAQ Ctrl+S, Ctrl+O, Ctrl+Q. |
| app_bootstrap.rs:20-25 | FAQ Ctrl+F, Ctrl+arrows, PgUp/PgDn, Home/End, Ctrl+Home/End. |
| app_bootstrap.rs:28-37 | FAQ Ctrl+W/Z/Y/X/C/V, Ctrl/Cmd+Shift+V, Ctrl+A, Ctrl+Bksp, Ctrl+Del. |
| app_bootstrap.rs:40-41 | FAQ F1 and F8. |
| render_view/ui.rs:1471 | Literal Ctrl+O — открыть файл. |
| render_view/settings_tool_rows.rs:485 | Literal «Ускорение Ctrl + колесо» labels Ctrl-wheel speed setting, not a command chord. |
| render_view/ide_panels/ide_panel_project_search_renderer.rs:519 | Справка поиска по проекту «Literal-only. Ctrl+Enter или кнопка запуска.» — подпись search.project.run (не SQL). |
| app_bootstrap.rs:17 | FAQ «Ctrl + Q Выйти из редактора (закрыть документ)» — покрывает app.quit и tabs.close_all. |
| Settings shortcut UI | Recon search found no key capture/record path (hotkeys.md:121). |

FAQ не документирует лаксовые варианты.

## 8. Новые команды C1–C24

Spec list has 24 commands, no defaults (docs/superpowers/specs/2026-10-03-hotkeys-design.md:138-149). IDs and labels below are proposed. Reuse UI handler methods; index-taking actions must resolve active object per spec:130.

| id | Russian label — context | Button handler evidence | Applicability; Unavailable |
|---|---|---|---|
| editor.git_diff.prev_hunk | «Предыдущий Git hunk» — Editor | InlineGitPrevHunk: ui_editor.rs:137; id ui_ids.rs:422 | Active doc has hunks; «Нет изменений Git в документе». |
| editor.git_diff.next_hunk | «Следующий Git hunk» — Editor | InlineGitNextHunk: ui_editor.rs:140; id ui_ids.rs:423 | Same. |
| git.stage_all | «Добавить всё в stage» — Global | GitStageAll(idx) calls stage_all_git_workspace: ui_git.rs:135-141; id ui_ids.rs:349 | Active repo; «Нет активного репозитория». |
| git.unstage_all | «Убрать всё из stage» — Global | GitUnstageAll(idx) opens confirmation: ui_git.rs:143-149; id ui_ids.rs:350 | Active workspace; same message. |
| git.push | «Git: отправить (push)» — Global | GitPush(idx) calls push_git_workspace: ui_git.rs:119-125; id ui_ids.rs:347 | Active repo/remote; «Нет удалённого репозитория». |
| git.fetch | «Git: получить (fetch)» — Global | GitFetch(idx) calls fetch_git_workspace: ui_git.rs:162-168; id ui_ids.rs:352 | Active repo; «Нет активного репозитория». |
| git.pull | «Git: вытянуть (pull)» — Global | GitPull(idx) calls pull_git_workspace: ui_git.rs:170-176; id ui_ids.rs:353 | Active repo/upstream; «Нет upstream-ветки». |
| git.refresh | «Обновить Git» — Global | GitRefresh calls refresh_git_panel_window: ui_git.rs:192-198; id ui_ids.rs:357 | Workspace exists; else «Нет активного репозитория». |
| git.toggle_graph | «Показать/скрыть граф Git» — Global | GitGraphToggle calls toggle_git_graph: ui_git.rs:200-205; id ui_ids.rs:358 | IDE mode; else «Доступно в IDE». |
| git.toggle_logs | «Показать/скрыть журнал Git» — Global | GitLogsToggle calls toggle_git_logs: ui_git.rs:207-212; id ui_ids.rs:359 | IDE mode; else «Доступно в IDE». |
| lsp.restart_server | «Перезапустить LSP» — Editor | LspServerRestart(idx) resolves server then restart_server: ui_lsp.rs:11-17; id ui_ids.rs:76 | Server for active doc; «Нет LSP-сервера активного документа». |
| lsp.fix_all | «Исправить всё (LSP)» — Editor | LspServerFixAll(idx), active path/fix request: ui_lsp.rs:105-121; id ui_ids.rs:81 | Server + writable doc; «Исправления недоступны». |
| api.import_openapi_file | «Импортировать OpenAPI-файл» — ApiClient | ApiImportFile opens picker: api_client_app_click_methods.rs:262-265; id ui_ids.rs:92 | API Client available; «API Client не открыт». |
| api.send_request | «Отправить запрос» — ApiClient | ApiTryRequest starts active request: api_client_app_click_methods.rs:735-737; id ui_ids.rs:122 | Active route/input; «Нет активного запроса». |
| api.mock.toggle_server | «Запустить/остановить API Mock» — ApiClient | ApiMockServerToggle: api_client_app_click_methods.rs:280-282; id ui_ids.rs:158 | Mock configuration; «API Mock не настроен». |
| api.mock.export_openapi | «Экспортировать OpenAPI» — ApiClient | ApiMockExportOpenApi triggers export: api_client_app_click_methods.rs:424-426; id ui_ids.rs:183 | Mock configuration; same. |
| database.refresh_selected | «Обновить подключение БД» — Database | DatabaseRefresh calls refresh_selected_database: ui_database.rs:70; id ui_ids.rs:239 | Selected connection; «Не выбрано подключение БД». |
| database.table.refresh | «Обновить таблицу» — Database | DatabaseTableRefresh active tab: ui_database.rs:309-312; id ui_ids.rs:283 | Active table; «Таблица не открыта». |
| database.table.save | «Сохранить изменения таблицы» — Database | DatabaseTableSave active tab: ui_database.rs:299-302; id ui_ids.rs:281 | Pending edits; «Нет изменений таблицы». |
| database.table.preview_sql | «Предпросмотр SQL изменений» — Database | DatabaseTablePreview active tab: ui_database.rs:304-307; id ui_ids.rs:282 | Active table; «Таблица не открыта». |
| database.query.explain | «План запроса (EXPLAIN)» — Database | DatabaseQueryExplain: ui_database.rs:484-486; id ui_ids.rs:317 | Active SQL query; «SQL-консоль не активна». |
| database.query.explain_analyze | «План с анализом» — Database | DatabaseQueryExplainAnalyze: ui_database.rs:487-488; id ui_ids.rs:318 | Active SQL query; same. |
| database.query.format | «Форматировать SQL» — Database | DatabaseQueryFormat: ui_database.rs:489; id ui_ids.rs:319 | Active console; same. |
| database.query.next_diagnostic | «Следующая ошибка SQL» — Database | DatabaseQueryNextDiagnostic: ui_database.rs:491-493; id ui_ids.rs:321 | Active console/diagnostic; «Нет ошибок SQL». |

Recon also suggested 3 Settings commands (hotkeys-commands.md:62-66); excluded because absent from normative C1-C24 list.

## 9. Имена клавиш

Parser key table is src/app/keyboard/key_input.rs:50-111. Defaults here need letters/digits/punctuation, Enter, Space, F1/F2/F8, Insert/Delete and NumpadEnter.

| Token | Status and evidence |
|---|---|
| numpadenter | Missing; SQL route uses NumpadEnter (main_keys.rs:383-384), no parser arm in key_input.rs:50-111. |
| pause | Missing; spec requires extension and permits bare Pause (design spec:51,62). |
| delete | Present: key_input.rs:78. |
| f2 | Present: key_input.rs:90. |
| insert | Present: key_input.rs:88. |
| f13-f24 | Not parsed (key_input.rs:89-100), no default requires them; image filter excludes F1-F24 (image_tab.rs:191-200). |
| numpad digit names | Не нужны: ни одна ветка не матчит Numpad0-9 (поиск `KeyCode::Numpad` в src/app: только NumpadEnter и NumpadDecimal в editor_keys.rs:78). |
| slash | Токена `slash` нет, есть `/` (key_input.rs:103); умолчание toggle_line_comment писать `mod+/`. |

## 10. Действие зависит от исходной клавиши

| Site | Verified source | Requirement |
|---|---|---|
| Alt+Q | Shift passed into helper (main_keys.rs:265-275); helper branches on shift (main_keys.rs:3-28). | Split terminal.toggle_focus alt+q and terminal.close alt+shift+q. |
| Tab switch | switch_tab_from_keyboard matches PageDown => next, PageUp => previous; rejects other keys (app_ide_tab_methods.rs:716-721). | Separate tabs.switch_next mod+pagedown and tabs.switch_previous mod+pageup. |
| Tree clipboard | KeyC/X/V map to three operations (file_tree_dialog.rs:489-501). | Separate copy/cut/paste IDs. |
| Slash comment | line comment marker receives extension, key and ctrl (editor_keys.rs:677-679); Slash branch acts on it (:1057-1069). | Semantic command, preserve default trigger. |
| Ctrl+Q | IDE vs non-IDE chooses CloseAllTabs or CloseFile (editor_keys.rs:719-734); welcome → quit (:455-483). | tabs.close_all (ветвление по режиму, не по клавише) + app.quit в Welcome; см. §1. |
| DB cell Enter | Enter/NumpadEnter → commit_database_table_cell_editor(tab_id, primary) (database_table_edit_methods.rs:1169-1174); флаг = `literal` (:419-428). | Enter фиксирован; Ctrl+Enter — команда database.table.commit_cell_literal — «Записать значение ячейки буквально» — Database — mod+enter, mod+numpadenter. |
| Search Enter | Shift решает направление (keyboard.rs:585-597, 723-737). | Фиксированные Enter/Shift+Enter (§2). |
| SQL Enter | Ctrl+Enter/NumpadEnter runs (main_keys.rs:383-393); review Enter commits (main_keys.rs:244-262). | Keep plain Enter state-specific and fixed. |

## 11. Фильтры — отдельный контракт каждого

| Filter | Position relative to chord and source | Protects / translation | Consumer |
|---|---|---|---|
| Startup gate | До диспатча (main_keys.rs:163-167). | Документ/вкладки во время старта; терминал печатает. | Main dispatcher. |
| PDF/image hidden editor | До центрального match (editor_keys.rs:492-496 < :718). | Скрытый редактор: Save/Edit не доходят. | Editor dispatcher. |
| Git diff readonly | До центрального match (editor_keys.rs:680-709). | cut/paste/comment заблокированы и после переназначения. | Editor dispatcher. |
| Markdown read | До автодополнения и match (editor_keys.rs:538-614). | Read-режим не редактирует. | Editor dispatcher. |
| Multicursor | До центрального match (editor_keys.rs:506-533). | apply-to-all для paste/undo/redo по команде. | Editor dispatcher. |
| PDF key ownership | Panel handler, только при editor_has_input_focus (pdf_tab/input.rs:7-9; main_keys.rs:404). | Команды проходят; pdf.copy локально. | PDF dispatcher. |
| Image key ownership | Reject modifiers/F1-F24 then Digit0 (image_tab.rs:184-214). | Reset-fit — фиксированная локальная ветка (§1), не команда. | Image dispatcher. |
| Tree filter | Focus/settings/selection guards (file_tree_dialog.rs:445-489). | Preserve context and selection guards when commands replace keys. | Main dispatcher calls helper at main_keys.rs:719-722. |
| DB table filter | Table handler before final route (main_keys.rs:404-405); focus.is_none for copy/undo/delete/insert (database_table_edit_methods.rs:1178-1189). | Preserve active table/no-input guard; Delete фиксированный, Insert — команда (§1). Нет проверки фокуса терминала (§4). | DB dispatcher. |
| API catch-all | Before final route (main_keys.rs:775-777); API tab consumes without form focus except F2 (api_client_app_request_methods.rs:408-415). | Dispatch API context commands before catch-all but after text input route. | API Client. |

См. §3.

## 12. Владелец ввода для шлюза терминала

Helper terminal_owns_keyboard_context (src/app/keyboard/main_keys.rs:74-103): false вне IDE или при закрытом терминале (:81-83); false, если владеет show_settings, file-tree rename/create, фокус project search, LSP filter, Git message, API field или LSP logs (:85-94); true при term_show_search && term_search_focused (:96-98); false при show_search && search_focused (:99-101); иначе terminal_focused (:102).

File-tree text owner itself returns !hard_modal_open && (rename_dialog.is_some() || create_dialog.is_some()); hard modals include API mock delete/reset, Git confirm, file-tree menu/move/delete (main_keys.rs:109-118). Helper is used by Markdown shortcut at :120-139. Final route goes terminal search, editor search, terminal, else editor (main_keys.rs:841-854); terminal search is a distinct handler but still terminal input owner.

| Input state | Ctrl+C | Ctrl+V | Ctrl+Shift+F | global Ctrl+A |
|---|---|---|---|---|
| Terminal focused | Ctrl+C sends 0x03; primary+C copies selection (keyboard.rs:527-538). | primary+V paste (keyboard.rs:506-522). | Project search branch precedes final terminal route (main_keys.rs:698-709): existing default exception. | Ctrl+A maps byte 0x01 (keyboard.rs:123-125). |
| Terminal search focused | helper true; handler копирует выделение поля поиска (keyboard.rs:646-650), не байт 0x03. | Вставка в поле поиска (keyboard.rs:658-663). | Project-search branch precedes terminal search unless earlier overlay returns (main_keys.rs:698-709,841-846). | Select-all поля поиска (keyboard.rs:643-645). Для шлюза: «терминал в фокусе» должен быть ложен при term_search_focused, иначе Ctrl+C/V/A уйдут в PTY — предикат шлюза ≠ terminal_owns_keyboard_context (:96-98 возвращает true). |
| Settings over terminal | helper false on show_settings (:85-94); Settings returns before globals (:662-668). | Settings/form owns. | Doesn't reach project-search branch. | Settings/form owns. |
| Dialog over terminal | Порядок: TOC (:180) → installer log (:198) → file-tree/API/Git modals (:240) → DB review (:247) → Alt+Q (:265) → confirm_dialog (:289) → DB prompts/DDL hover/help (:410-495). Все возвращают до терминала при любом фокусе. | Диалог владеет (или глушит). | Не доходит: все модалки выше :698. Alt+Q проходит поверх confirm_dialog (стоит раньше :289). | Диалог владеет; шлюз должен стоять после этих возвратов. |
| Text field over terminal | Git/API/search/LSP focus makes helper false (:85-103); поле владеет. | Поле. | Search route precedes terminal. | Поле (select-all). |

Task 5 must use same effective predicate for gateway and final router: финальный маршрут отдаёт PTY только при `is_ide_mode && terminal_focused && Terminal open && !(term_show_search && term_search_focused) && !(show_search && search_focused)` и после всех возвратов выше (main_keys.rs:841-852). Диалоги из таблицы выше перехватывают раньше; Settings тоже (:662-668, терминал не получает клавиши при открытых настройках).
