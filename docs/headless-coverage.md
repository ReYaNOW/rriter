# RRiter headless UI test coverage

Статическая инвентаризация исходников, без запуска тестов. В src/headless на 2026-09-27 найдено 283 теста (`rg -c '^\s*fn headless_' src/headless`); §1 ведёт только часть файлов, остальные ui_tests_*.rs перечислены в PROJECT_GUIDE.md §4. Не относящиеся к окну проверки помечены «нет окна». Если тест не меняет scale, указано значение по умолчанию 1.0 (src/headless/profile.rs:31-35).

## 1. Инвентарь тестов

### Editor — src/headless/ui_tests_editor.rs

| Тест и строка | Проверка | Окно @ scale |
|---|---|---|
| headless_editor_selection_and_cursor_at_multiple_scales (стр. 10) | Выделение и изменение положения курсора после Backspace. | 900×600 @ 1.0, 1.5 |
| headless_editor_drag_selection_and_fold_unfold (стр. 30) | Выделение перетаскиванием; сворачивание внешней функции скрывает вложенную стрелку, разворачивание возвращает её. | 1000×700 @ 1.0 |
| headless_editor_minimap_and_long_scroll_record_converge (стр. 73) | Клик по миникарте меняет scroll_y; запись колёсной прокрутки и PageDown не даёт обратных кадров. | 900×600 @ 1.0, 1.5 |
| headless_editor_search_overlay_and_completion (стр. 92) | Ctrl+F открывает поиск, Escape закрывает; Ctrl+Space активирует автодополнение. | 640×400 @ 1.0, 1.5; completion @ 1.0 |
| headless_editor_search_match_advances_and_escape_closes_at_both_scales (стр. 112) | Enter перемещает курсор к следующему совпадению; Escape закрывает поиск. | 640×400 @ 1.0, 1.5 |
| headless_editor_settings_overlay_closes_on_outside_click (стр. 131) | F1 открывает Settings, клик снаружи закрывает. | 640×400 @ 1.0, 1.5 |
| headless_editor_drag_selection_does_not_require_double_click (стр. 145) | Обычное перетаскивание мышью создаёт диапазон выделения. | 900×600 @ 1.0 |
| headless_editor_record_wheel_and_page_down_settle_without_reverse_frames (стр. 174) | Колесо и PageDown прокручивают длинный файл с монотонным движением кадров. | 1280×800 @ 1.0, 1.5 |
| headless_bug_fold_arrow_top_visible (стр. 190) | Верхняя граница: в начале длинного файла нет ложной sticky line или стрелки сворачивания. | 1920×1080 @ 1.0 |
| headless_bug_editor_text_body_minimum_width (стр. 208) | В компактном окне ширина тела редактора не меньше 150 px, миникарта скрыта. | 400×300 @ 2.0 |
| headless_bug_long_editor_tab_visible_close (стр. 220) | У вкладки с очень длинным именем остаётся зарегистрирована кнопка закрытия. | 1280×800 @ 1.5 |

### Layout — src/headless/ui_tests_layout.rs

| Тест и строка | Проверка | Окно @ scale |
|---|---|---|
| headless_layout_welcome_file_and_tree_size_scale_matrix (стр. 10) | Режимы welcome/editor/IDE сообщают ожидаемые размер и scale и сходятся после settle. | 640×480 и 2560×1440 @ 1.0, 1.25, 1.5, 2.0 |
| headless_layout_resize_and_scale_on_open_file (стр. 38) | Resize и смена scale сохраняют открытую вкладку и итоговые параметры окна. | старт 640×480 @ 1.0; 400×300 @ 1.0; 2560×1440 @ 1.0, 2.0, 1.25 |
| headless_layout_small_large_framebuffer_reports_exact_size_and_scale (стр. 55) | Дамп точно сообщает размеры буфера и все заданные масштабы. | 640×480 и 2560×1440 @ 1.0, 1.25, 1.5, 2.0 |
| headless_layout_resize_preserves_open_tab_and_scale_order (стр. 67) | Последовательное изменение размера и scale не теряет режим, путь и вкладку. | старт 900×600 @ 1.0; 640×480 @ 1.0, 1.25; 2560×1440 @ 1.25, 2.0, 1.0 |
| headless_layout_welcome_dump_has_no_file_tabs (стр. 85) | Пустой welcome режим не содержит вкладок и сохраняет заданный размер. | 640×480 и 2560×1440 @ 1.0 |
| headless_layout_file_and_tree_keep_requested_scale_through_resize (стр. 97) | Файл и IDE-дерево сохраняют scale после resize. | 640×480 и 2560×1440 @ 1.0, 1.25, 1.5, 2.0 |
| headless_layout_mode_survives_repeated_resize_cycle (стр. 130) | Режим редактора и открытая вкладка сохраняются при цикле размеров. | 640×480 → 400×300 → 2560×1440 → 640×480 → 1920×1080 @ 1.0 |
| headless_layout_minimum_size_resize_returns_ok_and_updates_dump (стр. 147) | Resize до компактного размера подтверждается и отражается в дампе. | 640×480 → 400×300 @ 1.0 |
| headless_layout_large_scale_scroll_record_and_4k_bench_summary (стр. 160) | Большой документ прокручивается без обратных кадров; 4K bench возвращает сводку. | 1280×800 @ 1.25, 1.5, 2.0; 3840×2160 @ 2.0 |
| headless_bug_welcome_rectangles_inside_compact_windows (стр. 183) | Hitbox welcome-кнопок остаются внутри компактного буфера. | 400×300 @ 1.0; 640×480 @ 2.0 |
| headless_bug_status_bar_stays_inside_with_bottom_panel (стр. 197) | Status bar не выходит за нижнюю границу при открытой нижней панели. | 1920×1080 @ 1.25 |

### Панели — src/headless/ui_tests_panels.rs

| Тест и строка | Проверка | Окно @ scale |
|---|---|---|
| headless_ide_panel_slots_open_and_register_controls (стр. 35) | Переключатели Explorer, Search, Git, API Client, Database, LSP Servers и Problems открывают панель с UI-контролами. | 1920×1080 @ 1.0 |
| headless_project_search_empty_and_open_result (стр. 92) | Запрос находит файл, переход по совпадению двигает курсор; новый запрос без результата скрывает строку файла. | 1920×1080 @ 1.0 |
| headless_database_dialog_and_api_mock_guide_controls (стр. 134) | При наличии контролов Database Add показывает поля; API Mock можно включить, увидеть URL и открыть/закрыть guide. | 1920×1080 @ 1.0 |
| headless_lsp_server_toggle_exposes_stop_control_when_available (стр. 159) | Для доступного сервера переключатель запускает его и при Running показывает Stop; без сервера тест выходит без проверки. | 1920×1080 @ 1.25 |
| headless_ide_panel_splitters_resize_side_and_bottom_panels (стр. 201) | Перетаскивание меняет ширину боковой панели; у нижнего разделителя сохраняется hitbox после drag. | 1920×1080 @ 1.0 |
| headless_terminal_scroll_returns_to_bottom_after_output (стр. 227) | Ввод команды создаёт вывод; крайняя прокрутка вниз возвращает терминал к тому же кадру, что и исходное дно. | 1920×1080 @ 1.0 |
| headless_bug_terminal_sidebar_slot_opens_panel (стр. 259) | Клик по Terminal открывает нижнюю панель после появления вывода shell. | 1920×1080 @ 1.0 |
| headless_bug_api_mock_guide_wheel_scrolls_content (стр. 281) | Колесо меняет изображение содержимого API Mock guide. | 1920×1080 @ 1.0 |
| headless_bug_sidebar_slots_hit_lsp_servers_at_small_sizes (стр. 317) | LSP Servers слот кликабелен в компактных окнах. | 640×480 @ 1.5; 400×300 @ 1.0 |
| headless_bug_git_changes_load_without_manual_refresh (стр. 329) | После открытия Git панели fixture показывает изменённые файлы без ручного refresh. | 1920×1080 @ 1.0 |
| headless_bug_settings_tab_y_integral (стр. 341) | Y и высота всех шести вкладок Settings целые; проверены края текстов и кнопки добавления папки. | 640×400 @ 1.5 |
| headless_bug_settings_help_right_edge_content (стр. 396) | В Settings → Help есть scrollbar, текст не доходит до его правой кромки. | 1280×800 @ 1.5 |

### Вкладки и дерево — src/headless/ui_tests_tabs_tree.rs

| Тест и строка | Проверка | Окно @ scale |
|---|---|---|
| headless_tabs_open_counts_and_active_close_switches_to_previous (стр. 23) | Открытие 1/5/20 вкладок обновляет счётчик; закрытие активной выбирает предыдущую. | 1280×800 @ 1.0 |
| headless_tabs_closing_inactive_tab_preserves_active_tab (стр. 62) | Закрытие неактивной вкладки оставляет активную вкладку и путь неизменными. | 1280×800 @ 1.0 |
| headless_tabs_dirty_close_dialog_discard_closes_tab (стр. 94) | Dirty вкладка через Ctrl+4 показывает запрос; Discard закрывает её. | 1280×800 @ 1.0 |
| headless_tree_250_files_expands_and_scrolls_within_budget (стр. 118) | Дерево из 250 файлов раскрывается; колёсная запись остаётся в frame budget. | 1280×800 @ 1.0 |
| headless_tree_long_filename_single_and_double_click (стр. 148) | Длинное имя файла выдерживает одиночный клик и открывается двойным. | 1280×800 @ 1.0 |
| headless_tree_search_and_file_open_fixture_paths_are_temporary (стр. 193) | Workspace показывает fixture узел и не открывает посторонние вкладки. | 900×600 @ 1.0 |
| headless_tree_expand_and_collapse_folder_restores_child_nodes (стр. 210) | Раскрытие добавляет дочерние узлы, сворачивание убирает их из UI. | 1280×800 @ 1.0 |
| headless_tree_record_wheel_keeps_frame_budget_at_common_scales (стр. 248) | Колёсная прокрутка дерева из 80 узлов укладывается в frame budget. | 1280×800 @ 1.0, 1.5 |
| headless_tabs_open_many_unique_fixture_files_without_path_leaks (стр. 265) | 20 уникальных fixture-файлов становятся вкладками; активна последняя. | 1920×1080 @ 1.0 |
| headless_tree_fixture_directory_can_be_opened_at_fractional_scale (стр. 286) | Workspace и узел дерева доступны при дробном scale. | 1280×800 @ 1.5 |
| headless_tree_dump_includes_scroll_track_for_large_fixture (стр. 300) | Для большого дерева регистрируется вертикальная полоса прокрутки. | 800×600 @ 1.0 |
| headless_tree_and_tabs_dump_use_only_scratch_fixture_paths (стр. 312) | Вкладка в дампе ссылается на scratch fixture файл. | 1024×768 @ 1.0 |

### Интеграция headless-сессии — src/headless/tests.rs

| Тест и строка | Проверка | Окно @ scale |
|---|---|---|
| headless_session_open_and_screenshot_png (стр. 123) | Открывает файл, сохраняет PNG нужного размера и проверяет фон/текст кадра. | 640×400 @ 1.0 |
| headless_session_open_rejects_missing_and_directory (стр. 148) | Ошибки открытия отсутствующего файла, директории и несуществующего workspace. | 640×400 @ 1.0 |
| headless_session_resize_changes_screenshot_size (стр. 164) | После resize PNG получает новый размер. | 640×400 → 800×600 @ 1.0 |
| headless_session_quit_stops_reading (стр. 176) | Quit прекращает чтение следующих команд. | 640×400 @ 1.0 |
| headless_session_app_exit_ends_loop_after_current_response (стр. 184) | Сигнал App exit прекращает цикл после текущего ответа. | 640×400 @ 1.0 |
| headless_session_skips_blank_and_comment_lines_and_counts_errors (стр. 193) | Игнорирует пустую/комментарийную строку, сообщает неизвестную команду и продолжает. | 640×400 @ 1.0 |
| headless_session_non_utf8_line_reports_error_and_continues (стр. 201) | Ошибка UTF-8 не мешает обработать следующую команду. | 640×400 @ 1.0 |
| headless_session_screenshot_io_error_then_continues (стр. 209) | Ошибка записи скриншота возвращается, затем принимается команда мыши. | 640×400 @ 1.0 |
| headless_session_broken_pipe_writer_ends_loop (стр. 231) | Ошибка записи ответа в pipe завершает цикл. | 640×400 @ 1.0 |
| headless_session_input_commands_answer_ok (стр. 239) | Команды мыши, колесо, клавиша, Unicode-ввод, scale, settle и wait получают ответы. | 640×400 @ 1.0 → 1.5 |
| headless_session_unreadable_input_is_an_error_exit (стр. 252) | Нечитаемый источник сценария завершает с ошибкой. | 640×400 @ 1.0 |
| headless_session_idle_settle_converges_without_frames (стр. 260) | Повторный settle сообщает, что кадры больше не нужны. | 640×400 @ 1.0 |
| headless_dump_has_all_keys_and_welcome_mode (стр. 281) | Dump содержит ключи состояния и UI реестр в welcome режиме. | 640×400 @ 1.0 |
| headless_dump_to_file_and_io_error (стр. 303) | Dump JSON записывается в файл; ошибка пути передаётся вызывающему. | 640×400 @ 1.0 |
| headless_dump_tracks_opened_tab (стр. 316) | Dump показывает активную вкладку, путь и число строк открытого файла. | 640×400 @ 1.0 |
| headless_dump_ui_rect_click_and_settings_key (стр. 332) | UI rect позволяет включить Markdown режим; F1 открывает Settings. | 1280×800 @ 1.0 |
| headless_dialog_cancel_then_discard_close_tab (стр. 355) | Диалог закрытия вкладки рисуется в кадре; Cancel сохраняет вкладку, Discard закрывает без записи. | 1280×800 @ 1.0 |
| headless_dialog_save_writes_file (стр. 391) | Save в диалоге подтверждения записывает изменение файла. | 1280×800 @ 1.0 |
| headless_dialog_without_dialog_errs (стр. 400) | Ответ dialog без открытого диалога возвращает ошибку. | 640×400 @ 1.0 |
| headless_dialog_escape_closes_like_window (стр. 407) | Escape закрывает запрос на закрытие dirty-файла. | 1280×800 @ 1.0 |
| headless_record_frames_csv_and_motion (стр. 420) | Запись сохраняет PNG кадров, CSV и метрики прокрутки. | 640×400 @ 1.0 |
| headless_record_io_error_then_continues (стр. 448) | Ошибка записи серии кадров не прерывает следующий dump. | 640×400 @ 1.0 |
| headless_bench_session_csv_summary_and_telemetry_restored (стр. 457) | Bench выдаёт CSV/сводку и восстанавливает telemetry flag. | 640×400 @ 1.0 |
| headless_bench_wheel_scrolls_long_document (стр. 488) | Bench с колесом фиксирует рост scroll_y длинного документа. | 640×400 @ 1.0 |
| headless_bench_csv_io_error_then_continues (стр. 508) | Ошибка CSV bench возвращается, последующий dump работает. | 640×400 @ 1.0 |
| headless_info_reports_budget_gl_and_policy (стр. 518) | Info сообщает бюджет кадров, GL, профиль и write policy. | 640×400 @ 1.0 |
| headless_protocol_fd_keeps_replies_and_moves_stray_output (стр. 545) | Разделяет протокольные ответы и посторонний вывод stdout/stderr. | Нет окна |

### Другие тесты в src/headless/

Парсер протокола — src/headless/protocol.rs:

| Тест и строка | Проверка | Размер и scale |
|---|---|---|
| headless_protocol_skips_blank_and_comment_lines (стр. 408) | Пустые строки и комментарии пропускаются. | Нет окна |
| headless_protocol_rejects_invalid_utf8_and_unknown_commands (стр. 417) | Невалидный UTF-8 и неизвестная команда отклоняются. | Нет окна |
| headless_protocol_parses_rest_of_line_paths (стр. 428) | Пути с пробелами читаются целиком. | Нет окна |
| headless_protocol_parses_resize_and_scale (стр. 442) | Разбирает resize 1920×1080 и scale 1.5. | Нет окна; значения только парсятся |
| headless_protocol_parse_size_bounds (стр. 456) | Проверяет границы формата WxH, включая 320×200. | Нет окна; значения только парсятся |
| headless_protocol_parses_mouse_commands (стр. 465) | Разбирает движение, кнопки и колесо мыши. | Нет окна |
| headless_protocol_rejects_bad_mouse_commands (стр. 480) | Отклоняет аргументы некорректных команд мыши. | Нет окна |
| headless_protocol_parses_key_and_type (стр. 498) | Разбирает комбинации клавиш и Unicode-ввод. | Нет окна |
| headless_protocol_parses_timing_commands (стр. 514) | Разбирает settle и wait. | Нет окна |
| headless_protocol_parses_dialog_info_quit (стр. 529) | Разбирает dialog, info и quit. | Нет окна |
| headless_protocol_parses_bench (стр. 543) | Разбирает параметры bench и действие. | Нет окна |
| headless_protocol_parses_record (стр. 566) | Разбирает параметры frame recording. | Нет окна |
| headless_protocol_response_line (стр. 579) | Формирует протокольные строки ok/err. | Нет окна |

Профиль — src/headless/profile.rs:

| Тест и строка | Проверка | Размер и scale |
|---|---|---|
| headless_profile_parse_defaults (стр. 314) | Значения CLI по умолчанию, включая scale 1.0. | Нет окна |
| headless_profile_parse_every_flag (стр. 328) | Разбирает полный набор флагов, включая размер и scale. | Нет окна; настройки только парсятся |
| headless_profile_parse_script_stdin_and_non_utf8_paths (стр. 362) | Разбирает stdin-сценарий и не-UTF пути. | Нет окна |
| headless_profile_parse_rejects_invalid_arguments (стр. 372) | Отклоняет некорректные параметры запуска. | Нет окна |
| headless_profile_parse_rejects_non_utf8_numbers (стр. 403) | Отклоняет не-UTF числовые значения флагов. | Нет окна |
| headless_profile_temp_root_is_private_unique_and_removed (стр. 412) | Временный корень приватен, уникален и удаляется. | Нет окна |
| headless_profile_temp_root_skips_foreign_directory (стр. 435) | Создание temp-root не перезаписывает чужую директорию. | Нет окна |
| headless_profile_keep_profile_and_dir_survive_finish (стр. 450) | Keep profile оставляет корень после finish. | Нет окна |
| headless_profile_from_user_copies_config_data_state_only (стр. 471) | Копирует только заданные каталоги пользовательского профиля. | Нет окна |
| headless_profile_from_user_skips_missing_sources (стр. 510) | Пропускает отсутствующие источники профиля. | Нет окна |

Кадры и dump — src/headless/frame.rs, src/headless/dump.rs:

| Тест и строка | Проверка | Размер и scale |
|---|---|---|
| headless_frame_flip_rows_swaps_2x2 (frame.rs:168) | Переставляет строки RGBA-буфера. | Нет окна |
| headless_frame_flip_rows_keeps_middle_row_and_ignores_short_buffer (frame.rs:178) | Сохраняет среднюю строку и безопасно обрабатывает короткий буфер. | Нет окна |
| headless_frame_settle_times_out_when_always_redrawn (frame.rs:188) | Завершает settle по тайм-ауту при постоянной перерисовке. | Нет окна |
| headless_frame_settle_two_idle_waits (frame.rs:200) | Считает два idle ожидания завершением settle. | Нет окна |
| headless_frame_settle_counts_redraws_between_idles (frame.rs:210) | Учитывает перерисовки между idle состояниями. | Нет окна |
| headless_frame_settle_wait_until_does_not_settle_and_respects_budget (frame.rs:221) | WaitUntil не считается idle и ограничен бюджетом. | Нет окна |
| headless_frame_write_png_creates_parent_and_forces_rgba (frame.rs:232) | PNG writer создаёт каталог и выдаёт RGBA. | Нет окна |
| headless_dialog_layout_centers_at_scale (dump.rs:296) | Рассчитывает центр диалога с масштабами 1.0/2.0 и 1.25. | Только входные геометрии: 1920×1080 @ 1.0, 2.0; 1000×500 @ 1.25 |
| headless_dialog_layout_smaller_buffer_starts_at_corner (dump.rs:304) | Малый буфер привязывает диалог к левому верхнему углу. | Только входная геометрия 320×200 @ 1.0 |
| headless_dump_external_request_kinds (dump.rs:309) | Сериализует виды внешнего запроса. | Нет окна |

Бенчмарк — src/headless/bench.rs:

| Тест и строка | Проверка | Размер и scale |
|---|---|---|
| headless_bench_resolve_budget_sources (стр. 486) | Выбирает источник бюджета кадров. | Нет окна |
| headless_bench_percentiles_nearest_rank (стр. 501) | Считает nearest-rank перцентили. | Нет окна |
| headless_bench_parse_proc_stat_cpu_ms (стр. 514) | Разбирает CPU время из proc stat. | Нет окна |
| headless_record_motion_stats (стр. 524) | Считает метрики движения scroll_y. | Нет окна |
| headless_bench_parse_loadavg (стр. 537) | Разбирает load average. | Нет окна |

## 2. Покрытие функций UI

Группы элементов взяты из UiId в src/ui_system/ui_ids.rs, панели — PanelId в src/app/app_state.rs:140-148, клавиатурная маршрутизация — src/app/keyboard/*.rs. Архитектурные группы сверены с PROJECT_GUIDE.md §2 и компактным индексом §4 (headless-строки 1457-1491). Маркер относится к проверяемому пользовательскому поведению; одного факта регистрации hitbox недостаточно для полного покрытия. `[~]` с «Отложено: …» — остаток сознательно не покрывается до названного условия (test-double, этап рефакторинга, фича); с 2026-09-27 новых пунктов покрытия ради покрытия нет.

### Welcome, настройки и overlays

- [x] Матрица экрана пользователя — 2560×1440, 1280×1440, 1280×720 @ 4/3 и smoke 640×480 @ 1.5 для welcome, файла, дерева, IDE-панелей и вкладок Settings: все hitbox внутри окна, y целые. Зелёные части: headless_layout_registered_hitboxes_welcome_size_matrix, headless_layout_registered_hitboxes_workspace_and_api_matrix, headless_layout_registered_hitboxes_smoke_file_explorer_and_lsp, headless_layout_registered_hitboxes_settings_tabs_that_fit_matrix (ui_tests_layout.rs:336,346,361,382). Полная матрица для редактора с длинной строкой, всех боковых панелей, API Client со спецификацией и всех вкладок Settings: headless_bug_*_size_matrix (ui_tests_layout.rs:407,421,435,475). Хитбоксы регистрируются привязанными к пикселям, как их рисует quad_vertices (UiClipRect::pixel_snapped в src/ui_system/ui_registry.rs); вкладка Settings «Базы данных» клипается к модалке, как вкладка инструментов. SettingsTab(3) «за низом окна» был кадром анимации выезда: тест ждёт settled=true. Terminal открывается после готовности PTY (wait 8000), карточка спецификации при 1280×720 достигается прокруткой панели. Состояние API Client/API Mock из общих /tmp-каталогов тестов сбрасывается перед каждым тестом (tests.rs reset_api_test_state). Высота hitbox не проверяется: строки дерева при 4/3 имеют h=37.333.
- [x] Welcome экран — New File создаёт вкладку, IDE Mode открывает IDE layout, Recent File открывает файл и удаляет отсутствующую запись; Open File выбирает файл через test picker, отмена сохраняет вкладки: headless_welcome_new_file_creates_untitled_tab, headless_welcome_ide_mode_enters_ide_layout, headless_welcome_recent_file_opens_seeded_file, headless_welcome_deleted_recent_file_is_removed_without_crash (ui_tests_welcome.rs:20,33,46,67), headless_welcome_open_file_uses_picker_answer, headless_welcome_open_file_picker_cancel_keeps_tabs_unchanged (ui_tests_pickers.rs:75,180). Размеры и hitbox: headless_layout_welcome_dump_has_no_file_tabs, headless_bug_welcome_rectangles_inside_compact_windows (ui_tests_layout.rs:87,185).
- [x] Диалог закрытия dirty-файла/вкладки — Save, Discard, Cancel и Escape проверены; кадр с диалогом отличается: headless_dialog_cancel_then_discard_close_tab, headless_dialog_save_writes_file, headless_dialog_escape_closes_like_window (tests.rs:355,391,407). Для dirty-вкладки IDE — см. пункт «Вкладки: open/close».
- [x] Settings overlay — F1 открывает, внешний клик закрывает; содержимое вкладок — пункты Settings ниже: headless_editor_settings_overlay_closes_on_outside_click, headless_dump_ui_rect_click_and_settings_key (ui_tests_editor.rs:131, tests.rs:332).
- [x] Settings → Редактор (индекс 2) — единственный контрол, множитель Ctrl+колеса: шаг, возврат, границы 1.25–5.0: ui_tests_settings_appearance.rs:48-110. Геометрия вкладок: headless_bug_settings_tab_y_integral (ui_tests_panels.rs:341).
- [x] Settings → Help — вкладка нажимается, scrollbar присутствует, текст не заходит на правый край: headless_bug_settings_help_right_edge_content (ui_tests_panels.rs:396). Колесо прокручивает с ограничением на обоих концах, drag scrollbar ограничен снизу, позиция прокрутки сохраняется при переключении вкладок, Escape закрывает overlay, содержимое внутри панели при 2560×1440: headless_settings_help_wheel_clamps_at_both_ends, headless_settings_help_drag_scrollbar_clamps_at_bottom, headless_settings_help_scroll_survives_tab_switch, headless_settings_help_escape_closes_overlay, headless_settings_help_large_window_content_stays_inside_panel (ui_tests_settings_help.rs:87,131,155,191,200).
- [x] Settings → IDE — добавление/удаление ignore pattern, отказ пустого/дубликата, прокрутка переполненного списка, удаление workspace и Add Workspace через test picker: ui_tests_settings_ide.rs:39-118, headless_settings_ide_add_workspace_uses_picker_answer (ui_tests_pickers.rs:108). Строки инструментов и Dart Restart/LSP log находятся на вкладке «Основные» — пункт «Установка инструментов и Dart-настройки».
- [~] Settings → Основные — Dart: поддержка, анализ workspace, closing labels, пороги nesting/block lines с возвратом; колесо прокручивает вкладку до Dart-контролов при 1280×720 @ 4/3 (баг: колесо работало только на вкладках IDE/Помощь → своя прокрутка и scrollbar у «Основные» и «Базы данных», kanri otb60yrpribdnv0q05nom36q): ui_tests_settings_general.rs:28-110. Refresh, clear override, лог установки, Dart Restart/LSP log — пункт «Установка инструментов и Dart-настройки». Не покрываем: Dart LSP/tooling — в headless-среде нет Dart LSP/tooling; tool pick/install, open directory, copy diagnostics также зависят от нативных диалогов, реального установщика и внешних процессов без test-double.
- [x] Settings → Базы данных — таймауты 0–4 и лимиты 5–8 ±1 с возвратом (у таймаута транзакции шаг 30 с), границы min/max, цвет подключения по кругу, последняя строка достижима колесом при 1280×720 @ 4/3: ui_tests_settings_database.rs:57-195.
- [~] Settings → Внешний вид — не покрываем: экран только визуальный, проверяется снимком и глазами; проверки пикселей хрупкие.
- [x] Установка инструментов и Dart-настройки Settings — Refresh перепроверяет Dart SDK, очистка override uv возвращает автообнаружение, лог установки копируется и закрывается кнопкой/Escape/backdrop, Dart Restart и «LSP log» в IDE открывают логи dart, «LSP log» вне IDE переводит в IDE и открывает панель LSP (баг: панель не открывалась → enter_ide_mode, kanri n39ew0cvmtz5d0ut7p52895z), кнопки строки инструмента не закрывают имя/путь/версию (баг раскладки settings_tool_rows.rs, kanri e8hzcq9n57glybzglak7ie2o): headless_settings_tools_refresh_rechecks_dart_sdk, headless_settings_tools_clear_override_restores_autodetection, headless_settings_tools_install_log_copies_and_closes, headless_settings_tools_dart_restart_and_open_log_in_ide, headless_settings_tools_dart_open_log_outside_ide_shows_lsp_panel, headless_settings_tools_row_buttons_leave_room_for_tool_status (ui_tests_settings_tools.rs:47,84,118,154,196,216). Не покрыты Cancel и «Открыть лог» установки — появляются только при реальной установке через сеть; Pick — нативный диалог, а глобальная ячейка external request флакала бы в параллельных тестах. UiId src/ui_system/ui_ids.rs:56-76.
- [x] Контекстные меню и File Tree create/rename/move/delete — меню на файле, папке и пустом месте (пункты, границы, Escape/внешний клик), создание файла и папки, отказ для пустого, существующего имени и имени с '/', rename с открытой вкладкой, drag-move с обновлением вкладки и отказ переноса папки в себя/потомка, диалог удаления с Cancel проверены при 1280×720 @ 4/3: ui_tests_tree_ops.rs:105-317. Подтверждённое удаление файла и непустой папки, undo удаления, вкладка удалённого файла остаётся открытой: ui_tests_tree_trash.rs (корзина под cfg(test) — temp-каталог по PID, platform.rs trash_layout). Что делать со вкладкой удалённого файла — kanri tcywredye9oo7jhm17nqx03u.
- [x] Удалённые файлы во вкладках — удаление из дерева закрывает чистую вкладку, грязная остаётся помеченной до undo, внешнее удаление ставит отметку, сохранение восстанавливает файл, внешнее создание снимает отметку и перезагружает вкладку: headless_tree_delete_closes_clean_tab, headless_tree_delete_keeps_dirty_tab_marked_until_undo, headless_external_rm_marks_clean_tab, headless_save_recreates_deleted_file_and_clears_mark, headless_external_recreate_clears_mark_and_reloads_clean_tab (ui_tests_deleted_tab.rs:42-107).
- [x] Hover popup и LSP hover/diagnostic popup — безусловно, на fake Ty (scripts/fake_lsp_server.py, режимы по имени файла `_long`/`_diagnostics`/`_crash`; tests_support::install_fake_ty): появление после задержки, релевантный текст, скрытие по уходу мыши и Escape, границы у правого/нижнего края, прокрутка длинной документации, copy у diagnostic popup: headless_lsp_hover_popup_* (ui_tests_panels.rs:311,343,386,427). Целый y hitbox попапа и скрытие попапа вводом текста (до движения мыши): headless_bug_hover_popup_hitbox_y_is_pixel_aligned, headless_bug_typing_hides_lsp_hover_popup (ui_tests_panels.rs:487,517). Сервер без ответа или с ошибкой на definition — попап всё равно показывается (ожидание ≤0,25 с, HOVER_DEFINITION_WAIT_SEC): headless_lsp_hover_popup_shows_when_definition_is_never_answered, …_definition_answers_with_error (ui_tests_panels.rs).
- [x] Обычный поиск в редакторе — Ctrl+F, ввод, Enter к совпадению, Escape (ui_tests_editor.rs); кнопки prev/next и Shift+Enter с переходом через край, case toggle, пустой запрос и нет совпадений (ui_tests_editor_search.rs). Replace в редакторе нет — Search* UiId в src/ui_system/ui_ids.rs.
- [x] Центрирование совпадения поиска — найденная строка остаётся по центру окна при 2560×1440 и 1280×720: headless_editor_search_match_stays_centered_at_2560x1440, headless_editor_search_match_stays_centered_at_1280x720 (ui_tests_editor_search_center.rs:78-84).

### IDE панели и сервисы

- [x] Sidebar slots — проверено переключение Explorer, Search, Git, API Client, Database, LSP Servers и Problems с появлением контролов; Terminal проверен отдельно: headless_ide_panel_slots_open_and_register_controls, headless_bug_terminal_sidebar_slot_opens_panel (ui_tests_panels.rs:35,259; PanelId src/app/app_state.rs:140-148).
- [x] File Tree просмотр — раскрытие/сворачивание, открытие длинного имени, большой список, полоса прокрутки и дробный scale проверены; операции с файлами — отдельный пункт выше: headless_tree_expand_and_collapse_folder_restores_child_nodes, headless_tree_long_filename_single_and_double_click, headless_tree_250_files_expands_and_scrolls_within_budget (ui_tests_tabs_tree.rs:194,132,102).
- [x] Project Search — поиск и пустой результат, include/exclude glob, case toggle, help, live path filter, переход к совпадениям во втором и третьем файле и прокрутка списка из многих совпадений: headless_project_search_empty_and_open_result (ui_tests_panels.rs:92), headless_project_search_include_and_exclude_globs_narrow_files, headless_project_search_case_help_and_live_path_filter_controls, headless_project_search_second_and_third_file_results_open_at_match_line, headless_project_search_result_list_scrolls_through_many_matches (ui_tests_project_search.rs:77,105,161,207; UiId src/ui_system/ui_ids.rs:390-403).
- [x] Git changes — список изменённых файлов появляется без ручного refresh, stage/unstage чекбоксом проверен по `git diff --cached`: headless_bug_git_changes_load_without_manual_refresh (ui_tests_panels.rs), headless_git_file_can_be_staged_and_unstaged (ui_tests_git_commit.rs:41). Git UiId — src/ui_system/ui_ids.rs.
- [x] Git graph и commit history — merge-история отрисовывается с hitbox строк, hover/клик по commit показывает карточку и копирует полный hash, длинная история прокручивается: ui_tests_git_graph.rs:70,89,111 (1280×720 @ 4/3). Отдельного состояния «выбранный commit» нет — клик по GitGraphCommit только перерисовывает (ui_handlers/ui_git.rs).
- [x] Git diff tabs/hunks — открытие diff двойным кликом по файлу, навигация вперёд/назад по трём hunk, откат одного hunk с сохранением (остальные изменения на диске целы), закрытие вкладки; hitbox rollback внутри окна после навигации (был баг: гаттер регистрировал hitbox строк overscan-запаса, исправлено отсечением по зоне редактора в root_frame_renderer.rs): ui_tests_git_diff.rs:83-157. Откат остальных hunk и циклический переход не покрыты.
- [x] Git commit/push flow — commit только staged-файла с введённым сообщением, пункты меню commit и skip hooks, отказ при пустом сообщении и скрытый commit без staged-файлов, push в локальный bare remote: ui_tests_git_commit.rs:59-128. Подтверждения push в UI нет (GitPush пушит сразу); Amend и Commit & Push не выполняются.
- [x] API Client — открытие endpoint, path/query параметры, Bearer/Basic/API key auth, GET/POST, status/body для ответов и ошибок, connection error и cURL copy: headless_api_client_spec_lists_and_opens_endpoint, headless_api_client_spec_edits_path_and_query_parameters, headless_api_client_spec_edits_bearer_and_basic_auth, headless_api_client_spec_edits_api_key_header_auth (ui_tests_api_client_spec.rs:171-245), headless_api_client_get_shows_status_and_body, headless_api_client_post_sends_json_body, headless_api_client_http_error_shows_status_and_body, headless_api_client_refused_connection_shows_error, headless_api_client_curl_view_can_be_copied (ui_tests_api_client_request.rs:228-289). Импорт OpenAPI по URL с локального HTTP-сервера добавляет спеку и маршруты, пустой и не-http URL отклоняется, HTTP 404 и невалидный JSON показывают ошибку, повторный импорт того же URL не дублирует спеку: ui_tests_api_client_import.rs:80-164; импорт через picker: headless_api_client_imports_openapi_from_picker (ui_tests_pickers.rs:26). Multipart, refresh/remove/filter/tag navigation и response views покрыты в ui_tests_api_client_multipart.rs, ui_tests_api_client_navigation.rs и ui_tests_api_client_response_views.rs. Импорт из файла и Save As проверены через test picker (ui_tests_pickers.rs:26,137).
- [x] API Mock — включение сервера показывает URL, guide открывается/закрывается и прокручивается: headless_database_dialog_and_api_mock_guide_controls, headless_bug_api_mock_guide_wheel_scrolls_content (ui_tests_panels.rs:134,281; UiId src/ui_system/ui_ids.rs:180-235). Редактирование route (путь/метод) в списке, mock-сервер отвечает по route реальным HTTP, неизвестный путь → 404 в режиме MockAll (по умолчанию MockSelectedProxyRest проксирует → 502), Stop закрывает порт, Python/contract controls следуют пути route: headless_api_mock_manual_route_can_be_edited_in_list, headless_api_mock_server_returns_manual_route_response, headless_api_mock_unknown_path_returns_404, headless_api_mock_stop_closes_listener, headless_api_mock_python_contract_controls_follow_route_path (ui_tests_api_mock.rs:170,187,213,240,257). Python-обработчик: результат отдаётся по HTTP, исключение → 500 и строка в логе, синтаксическая ошибка → 500, правка применяется после рестарта и на запущенном сервере без рестарта (баг: commit_api_focus не обновлял snapshot сервера → refresh_api_mock_server_snapshot, kanri uqv49sgwo1ywxqrhuy2cm7bq): headless_api_mock_python_handler_result_is_served, headless_api_mock_python_exception_returns_500_and_shows_in_log, headless_api_mock_python_syntax_error_returns_500, headless_api_mock_python_edit_changes_response_after_restart, headless_api_mock_python_edit_hot_updates_running_server (ui_tests_api_mock_python.rs). Контракт: path/query/JSON body доходят до обработчика, отсутствующий optional query → None, таймаут → 500 с «Python mock timeout» и сервер продолжает отвечать (баг: колесо не прокручивало combined editor ручного маршрута, тело обработчика было недостижимо → общий api_route_model_for_identity, kanri pxxbwpjz2uozjks4glc17305); timeout_ms и поля контракта засеваются в state (UI нет): headless_api_mock_python_handler_receives_path_query_and_json_body, headless_api_mock_python_handler_receives_missing_optional_query_as_none, headless_api_mock_python_timeout_returns_500_logs_timeout_and_server_recovers (ui_tests_api_mock_contract.rs:67,111,144).
- [x] Database panel — Database Add показывает поля формы, Escape закрывает: headless_database_dialog_and_api_mock_guide_controls (ui_tests_panels.rs:134). С PostgreSQL-фикстурой (scripts/postgres_fixture.py, общая с PGO): соединение через форму сохраняет настройки, каталог rriter_pgo/pgo_items, открытие таблицы со строками и колонками, ошибка закрытого порта: headless_database_connection_form_saves_fixture_settings, headless_database_fixture_catalog_opens_table_rows_and_columns, headless_database_closed_port_shows_connection_error (ui_tests_database_connect.rs:27,46,106); SELECT → сетка результата, EXPLAIN → план, ошибка SQL показывает текст сервера и консоль работает после неё (баг: было «db error» → Display DbError, kanri zhfbmu8vmnuao2dg7jhmtqou): headless_database_query_select_renders_result_grid, headless_database_query_explain_renders_plan, headless_database_query_ddl_error_keeps_console_usable (ui_tests_database_query.rs:112,138,162). UPDATE…RETURNING показывает возвращённые строки, успешный DDL оставляет консоль пригодной; редактирование ячейки сохраняется и перезагружается, изменение primary key показывает ошибку: headless_database_query_update_returning_renders_returned_rows, headless_database_query_ddl_success_keeps_console_usable, headless_database_table_cell_edit_saves_and_reloads (ui_tests_database_edit.rs:82,104,135). UiId src/ui_system/ui_ids.rs:236-330.
- [x] Terminal — команда и вывод, вкладки (добавить/переключить/закрыть), поиск с навигацией и закрытием, выделение мышью и копирование текста, прокрутка и возврат вниз: headless_terminal_types_command_and_shows_its_output, headless_terminal_tabs_add_switch_and_close, headless_terminal_search_highlights_navigates_and_closes, headless_terminal_mouse_selection_copies_selected_text (ui_tests_terminal.rs:79,95,139,201), headless_terminal_scroll_returns_to_bottom_after_output, headless_bug_terminal_sidebar_slot_opens_panel (ui_tests_panels.rs:227,259; UiId src/ui_system/ui_ids.rs:451-463).
- [x] LSP Servers — для настроенного фейкового сервера проверены start/Stop, переход Stop → Disabled и ограничение четырьмя попытками при падении без restart loop: headless_lsp_servers_panel_lists_configured_ty_and_starts_it, headless_lsp_servers_panel_stop_returns_ty_to_disabled, headless_lsp_servers_panel_reports_fake_ty_crash_without_restart_loop (ui_tests_lsp_servers.rs:51,80,107); также проверены toggle для доступного сервера и hitbox слота в компактных окнах (ui_tests_panels.rs:159,317).
- [x] Problems panel — список файла/строки/сообщения, переход к диагностике и удаление строки после очистки diagnostics (ui_tests_problems.rs:77-104); вкладки «текущий файл»/«все», сворачивание группы файла, две группы со счётчиками (ui_tests_problems_groups.rs:86-159); открытие URL диагностики в external request log (headless_problems_click_opens_diagnostic_url_in_external_request_log, ui_tests_problem_url.rs:36). Слот и resize hitbox: headless_ide_panel_slots_open_and_register_controls, headless_ide_panel_splitters_resize_side_and_bottom_panels (ui_tests_panels.rs:35,201; UiId src/ui_system/ui_ids.rs:444-445).
- [x] Splitters и status bar — ширина левой панели меняется, status bar остаётся в пределах окна: headless_ide_panel_splitters_resize_side_and_bottom_panels, headless_bug_status_bar_stays_inside_with_bottom_panel (ui_tests_panels.rs:201,197). Высота нижней панели меняется drag'ом и сохраняется после скрытия/показа, границы min/max нижней и боковой панели у краёв окна, status bar прижат к низу окна при двух размерах: headless_splitters_bottom_drag_changes_height_and_survives_toggle, headless_splitters_bottom_panel_stays_bounded_at_window_edges, headless_splitters_side_panel_stays_bounded_at_window_edges, headless_status_bar_stays_at_window_bottom_at_two_sizes (ui_tests_splitters.rs:55,83,109,133).
- [x] Scrollbars панелей — колесо и drag ограничивают прокрутку Git workspace и API Mock combined editor у нижней границы; клик по track терминала прокручивает назад к низу: headless_git_workspace_scrollbar_wheel_and_drag_clamp_at_bottom, headless_api_mock_combined_editor_scrollbar_wheel_and_drag (ui_tests_scrollbars_panels.rs:27,83), headless_terminal_scrollbar_track_click_returns_toward_bottom (ui_tests_terminal_scrollbar.rs:9).

### Редактор, клавиатура и вкладки

- [~] Клавиатурная маршрутизация — основные сочетания покрыты в ui_tests_editor_shortcuts.rs и ui_tests_keyboard_panels.rs; остальные не покрываем: это тонкая маршрутизация, отдельные UI-проверки малоценны. Проверенные editor shortcuts: ui_tests_editor.rs:10-220, ui_tests_editor_shortcuts.rs:15-80; panel shortcuts F1, Alt+Q/Shift+Alt+Q, Alt+W, Ctrl+Shift+F, Ctrl+4 и Escape: ui_tests_keyboard_panels.rs:18-179.
- [x] Editor shortcuts — Save (файл на диске, dirty снят), Undo с пустой историей, Undo, Redo по Ctrl+Y, Copy→Paste, Cut→Paste, Paste при пустом буфере, многострочный Copy→Paste и comment toggle на пустой, одной и нескольких строках проверены при 1280×720 @ 4/3: ui_tests_editor.rs:268,302,326,345,365,381. Home/End и Ctrl+←/→, удаление по словам с границами файла, Enter с автоотступом и на пустой строке, Tab вставляет 4 пробела: headless_editor_shortcuts_line_and_word_navigation, headless_editor_shortcuts_delete_words_and_respect_file_edges, headless_editor_shortcuts_enter_keeps_auto_indent_and_handles_empty_line, headless_editor_shortcuts_tab_inserts_four_spaces_on_empty_line (ui_tests_editor_shortcuts.rs:15,41,62,80). Ctrl+Shift+Z не привязан к Redo; Shift+Tab outdent в продукте нет.
- [x] Выделение и курсор — drag, Shift+Right и Backspace меняют selection/cursor: headless_editor_selection_and_cursor_at_multiple_scales, headless_editor_drag_selection_does_not_require_double_click (ui_tests_editor.rs:10,145). Двойной клик выделяет слово, Shift+Home/End, Ctrl+A, Ctrl+Shift+←/→ по словам, ввод заменяет выделение: headless_editor_double_click_selects_word, headless_editor_shift_home_and_end_expand_selection, headless_editor_ctrl_a_selects_entire_document, headless_editor_ctrl_shift_arrows_select_by_words, headless_editor_typing_replaces_selection (ui_tests_editor_selection.rs:24,50,63,72,87). Shift+клик не покрыт — в протоколе драйвера нет модификатора к клику.
- [x] Multi-cursor — Alt+click добавляет/удаляет caret, ввод выполняется на трёх строках и обновляет syntax spans, Backspace, Escape, plain click, undo/redo с восстановлением caret-группы, трёхстрочный paste и Shift+Right, снимающий extra carets: `ui_tests_multi_cursor.rs`.
- [x] Folding — внешняя функция скрывает вложенную fold arrow и раскрывает её обратно, у верхнего края нет ложной стрелки: headless_editor_drag_selection_and_fold_unfold, headless_bug_fold_arrow_top_visible (ui_tests_editor.rs:30,190). Свёрнутый блок скрывает строки и курсор их пропускает, соседние блоки сворачиваются независимо: headless_editor_folding_hides_rows_and_skips_cursor, headless_editor_folding_sibling_blocks_toggle_independently (ui_tests_editor_folding_minimap.rs:78,117).
- [x] Minimap — клик меняет scroll_y на двух scale: headless_editor_minimap_and_long_scroll_record_converge (ui_tests_editor.rs:73). Drag thumb пропорционален и ограничен, клик в нижней части ведёт к строке под курсором (minimap показывает окно ≤900 строк), геометрия thumb при 1280×720 и 2560×1440: headless_editor_minimap_drag_thumb_is_proportional_and_clamped, headless_editor_minimap_click_tracks_lower_file_position, headless_editor_minimap_thumb_geometry_at_two_viewports (ui_tests_editor_folding_minimap.rs:161,200,242). Баг: клик/drag считали minimap с 1.5 px/строку против 2.0 в отрисовке → общие minimap_view_metrics/minimap_thumb_height (kanri e7tgkwxq1i3wvui4g1e41n6h).
- [x] Sticky lines — нет у верха файла, при прокрутке появляется внешний блок, вложенные соседи сменяют друг друга, клик по sticky переводит курсор на заголовок; hitbox с целым y внутри окна при 1280×720 @ 4/3: ui_tests_editor_sticky.rs.
- [x] Markdown preview/Read mode — проверены layout заголовков, списков и code block, wheel scroll с ограничением на обоих концах, code copy и возврат в edit без изменения исходника: headless_markdown_read_lays_out_preview_blocks, headless_markdown_read_wheel_clamps_at_both_ends, headless_markdown_code_copy_and_edit_toggle_preserve_source (ui_tests_markdown.rs:101,141,214). Drag scrollbar: headless_markdown_read_scrollbar_is_present_and_draggable (ui_tests_markdown.rs:164; баг — сверка max_scroll отвергала drag при scale 4/3, допуск 1 px в src/app/markdown.rs:604).
- [x] Goto definition — Ctrl-hover подсвечивает точный символ и очищает цель, Ctrl-click переходит к определению в том же или другом файле и прокручивает к нему; клик вне символа не выполняет переход: headless_goto_definition_ctrl_hover_highlights_exact_symbol_and_release_clears_it, headless_goto_definition_ctrl_click_jumps_to_same_file_definition_and_scrolls, headless_goto_definition_ctrl_click_opens_definition_in_another_file, headless_goto_definition_moving_to_non_symbol_clears_target_and_click_does_not_navigate (ui_tests_goto_definition.rs:121,151,187,224).
- [x] Completion (Tree-sitter путь, без LSP) — список по префиксу и фильтрация вводом, Escape без вставки, стрелки, Enter/Tab применяют с курсором после вставки, клик: первый выбирает (виден docstring), второй применяет — так задумано; popup у правого/нижнего края внутри окна (был баг, исправлен в render_view/ui.rs autocomplete_popup_x): ui_tests_editor_completion.rs. LSP-completion и сниппеты не покрыты.
- [x] Вкладки: open/close — проверены счётчик, закрытие активной/неактивной вкладки и dirty confirm с Discard; drag и полоса вкладок — следующий пункт: headless_tabs_open_counts_and_active_close_switches_to_previous, headless_tabs_closing_inactive_tab_preserves_active_tab, headless_tabs_dirty_close_dialog_discard_closes_tab (ui_tests_tabs_tree.rs:78). Диалог dirty-вкладки: Save закрывает и сохраняет CRLF, Cancel оставляет текст и dirty-маркер, клик вне модального диалога не доходит до редактора, Escape отменяет (баг: Escape только прятал диалог, pending CloseTab переоткрывал его → Escape в main_keys.rs вызывает cancel_pending_action, kanri v5zsy50kdtflytjs7u7wtz01), после Ctrl+S вкладка закрывается без диалога: headless_tabs_dirty_close_dialog_save_closes_tab, headless_tabs_dirty_close_dialog_save_keeps_crlf, headless_tabs_dirty_close_dialog_cancel_keeps_text_and_dirty_marker, headless_tabs_dirty_close_dialog_ignores_click_outside, headless_tabs_dirty_close_dialog_escape_cancels, headless_tabs_close_after_ctrl_s_skips_dialog (ui_tests_tabs_dirty.rs:21,32,43,58,78,93).
- [x] Перетаскивание вкладок и прокрутка длинной полосы вкладок — drag активной вкладки меняет порядок и сохраняет содержимое, края ограничивают, клик без движения не переставляет, autoscroll у края при drag, колесо прокручивает полосу с ограничением на обоих концах, новая вкладка попадает в кадр, hitbox вкладок внутри окна с целым y при 1280×720 и 2560×1440 @ 4/3: ui_tests_tabs_tree.rs:396-537. Кнопок прокрутки у полосы нет (только edge fades). Drag неактивной вкладки активирует её и меняет порядок: headless_bug_dragging_inactive_editor_tab_reorders_and_activates_it (ui_tests_tabs_tree.rs:576). Переключение с клавиатуры Ctrl+PageDown/PageUp (Ctrl+Tab не привязан): по кругу на краях, активная вкладка прокручивается в кадр, работает из фокуса терминала, PageUp/PageDown без Ctrl уходят в терминал: headless_keyboard_activation_reveals_offscreen_tab, headless_keyboard_tab_switch_wraps_and_reveals_ends, headless_terminal_plain_page_keys_do_not_switch_editor_tabs (ui_tests_tabs_tree.rs:606-690).

### Размер окна и масштаб

- [x] Resize/scale состояние и layout — матрицы размеров, масштабы 1.0–2.0, циклы resize, компактные окна и 4K проверены в headless режиме: headless_layout_welcome_file_and_tree_size_scale_matrix, headless_layout_small_large_framebuffer_reports_exact_size_and_scale, headless_layout_file_and_tree_keep_requested_scale_through_resize, headless_layout_mode_survives_repeated_resize_cycle (ui_tests_layout.rs:10,55,97,130).
- [x] DPI-пара 2560×1440 @ 4/3 (логические 1920×1080), половина 1280×1440 и четверть 1280×720 при том же scale — матрица hitbox выше, вкладки, дерево, шорткаты и hover. Native OS DPI/window scaling этим не проверяется.

## 3. Уникальные размеры окна и scale

Фактические конфигурации HeadlessSession из UI/session тестов; повторы объединены. Все размеры здесь — размеры offscreen окна/буфера из сценариев.

- 400×300 @ 1.0, 2.0
- 640×400 @ 1.0, 1.5
- 640×480 @ 1.0, 1.25, 1.5, 2.0
- 800×600 @ 1.0 (конечный размер в resize-тесте tests.rs:164-172)
- 900×600 @ 1.0, 1.5
- 1000×700 @ 1.0
- 1024×768 @ 1.0
- 1280×720 @ 4/3
- 1280×1440 @ 4/3
- 1280×800 @ 1.0, 1.25, 1.5, 2.0
- 1920×1080 @ 1.0, 1.25
- 2560×1440 @ 1.0, 1.25, 4/3, 1.5, 2.0
- 3840×2160 @ 2.0

Дополнительно, только входы чистой функции dialog_layout, не окна: 1920×1080 @ 1.0 и 2.0; 1000×500 @ 1.25; 320×200 @ 1.0 (src/headless/dump.rs:296-307). Размер 800×600 указан как итог resize-сценария, а не исходный HeadlessSession (src/headless/tests.rs:164-172).

Проверки не запускались: задача запрещает сборки и тесты.
