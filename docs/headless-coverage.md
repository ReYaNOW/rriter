# RRiter headless UI test coverage

Статическая инвентаризация исходников, без запуска тестов. В src/headless найдено 111 тестов: 46 UI-регрессий, 27 тестов headless-сессии, 38 тестов вспомогательных модулей. Не относящиеся к окну проверки помечены «нет окна». Если тест не меняет scale, указано значение по умолчанию 1.0 (src/headless/profile.rs:31-35).

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

Группы элементов взяты из UiId в src/ui_system.rs:31-464, панели — PanelId в src/app/app_state.rs:140-148, клавиатурная маршрутизация — src/app/keyboard/*.rs. Архитектурные группы сверены с PROJECT_GUIDE.md §2 и компактным индексом §4 (стр. 1459-1464). Маркер относится к проверяемому пользовательскому поведению; одного факта регистрации hitbox недостаточно для полного покрытия.

### Welcome, настройки и overlays

- [x] Матрица экрана пользователя — 2560×1440, 1280×1440, 1280×720 @ 4/3 и smoke 640×480 @ 1.5 для welcome, файла, дерева, IDE-панелей и вкладок Settings: все hitbox внутри окна, y целые. Зелёные части: headless_layout_registered_hitboxes_welcome_size_matrix, headless_layout_registered_hitboxes_workspace_and_api_matrix, headless_layout_registered_hitboxes_smoke_file_explorer_and_lsp, headless_layout_registered_hitboxes_settings_tabs_that_fit_matrix (ui_tests_layout.rs:336,346,361,382). Полная матрица для редактора с длинной строкой, всех боковых панелей, API Client со спецификацией и всех вкладок Settings: headless_bug_*_size_matrix (ui_tests_layout.rs:407,421,435,475). Хитбоксы регистрируются привязанными к пикселям, как их рисует quad_vertices (UiClipRect::pixel_snapped в ui_system.rs); вкладка Settings «Базы данных» клипается к модалке, как вкладка инструментов. SettingsTab(3) «за низом окна» был кадром анимации выезда: тест ждёт settled=true. Terminal открывается после готовности PTY (wait 8000), карточка спецификации при 1280×720 достигается прокруткой панели. Состояние API Client/API Mock из общих /tmp-каталогов тестов сбрасывается перед каждым тестом (tests.rs reset_api_test_state). Высота hitbox не проверяется: строки дерева при 4/3 имеют h=37.333.
- [~] Welcome экран — тесты проверяют режим, отсутствие вкладок и границы hitbox, но не кликают New File, Open File, IDE Mode или Recent File: headless_layout_welcome_dump_has_no_file_tabs, headless_bug_welcome_rectangles_inside_compact_windows (ui_tests_layout.rs:87,185).
- [x] Диалог закрытия dirty-файла/вкладки — Save, Discard, Cancel и Escape проверены; кадр с диалогом отличается: headless_dialog_cancel_then_discard_close_tab, headless_dialog_save_writes_file, headless_dialog_escape_closes_like_window (tests.rs:355,391,407).
- [~] Settings overlay — F1 открывает, внешний клик закрывает; общая логика содержимого настроек этим не покрыта: headless_editor_settings_overlay_closes_on_outside_click, headless_dump_ui_rect_click_and_settings_key (ui_tests_editor.rs:131, tests.rs:332).
- [~] Settings → Editor (индекс 2) — только целочисленные y/height hitbox вкладок в тесте всех шести вкладок; элементы редакторских настроек не меняются: headless_bug_settings_tab_y_integral (ui_tests_panels.rs:341; названия вкладок render_view/settings_ui.rs:496).
- [~] Settings → Help — вкладка нажимается, scrollbar присутствует, текст не заходит на правый край; прокрутка содержимого не проверяется: headless_bug_settings_help_right_edge_content (ui_tests_panels.rs:396).
- [~] Settings → IDE — проверяется наличие Add Workspace hitbox и геометрия вкладок, но добавление/удаление workspace и ignore не выполняется: headless_bug_settings_tab_y_integral (ui_tests_panels.rs:341; UiId src/ui_system.rs:47-54).
- [ ] Settings → Основные, Внешний вид, Базы данных — тесты не меняют соответствующие значения; проверка шести вкладок касается только геометрии. Вкладки перечислены в render_view/settings_ui.rs:496.
- [ ] Установка инструментов и Dart-настройки Settings — pick/install/cancel/log и Dart support controls не упражняются headless-тестами: UiId src/ui_system.rs:56-76.
- [~] Контекстные меню и File Tree create/rename/move/delete — меню на файле, папке и пустом месте (пункты, границы, Escape/внешний клик), создание файла и папки, отказ для пустого, существующего имени и имени с '/', rename с открытой вкладкой, drag-move с обновлением вкладки и отказ переноса папки в себя/потомка, диалог удаления с Cancel проверены при 1280×720 @ 4/3: ui_tests_tree_ops.rs:105-317. Подтверждённое удаление не тестируется — оно идёт в реальную ~/.local/share/Trash (platform.rs trash_layout); нужен test-double Trash (kanri bf26z6jckzub0vzu2pzy1wj5).
- [~] Hover popup и LSP hover/diagnostic popup — при доступном ty/ruff: появление после задержки, релевантный текст, скрытие по уходу мыши и Escape, границы у правого/нижнего края, прокрутка длинной документации, copy у diagnostic popup: headless_lsp_hover_popup_* (ui_tests_panels.rs:311,343,386,427). Без LSP-сервера тесты деградируют; безусловных путей нет — hover всегда идёт через LSP. Целый y hitbox попапа и скрытие попапа вводом текста (до движения мыши): headless_bug_hover_popup_hitbox_y_is_pixel_aligned, headless_bug_typing_hides_lsp_hover_popup (ui_tests_panels.rs:487,517).
- [x] Обычный поиск в редакторе — Ctrl+F, ввод, Enter к совпадению, Escape (ui_tests_editor.rs); кнопки prev/next и Shift+Enter с переходом через край, case toggle, пустой запрос и нет совпадений (ui_tests_editor_search.rs). Replace в редакторе нет — Search* UiId в src/ui_system/ui_ids.rs.

### IDE панели и сервисы

- [~] Sidebar slots — проверено переключение Explorer, Search, Git, API Client, Database, LSP Servers и Problems с появлением контролов; Terminal проверен отдельно: headless_ide_panel_slots_open_and_register_controls, headless_bug_terminal_sidebar_slot_opens_panel (ui_tests_panels.rs:35,259; PanelId src/app/app_state.rs:140-148).
- [~] File Tree просмотр — раскрытие/сворачивание, открытие длинного имени, большой список, полоса прокрутки и дробный scale проверены; операции с файлами — отдельный пункт выше: headless_tree_expand_and_collapse_folder_restores_child_nodes, headless_tree_long_filename_single_and_double_click, headless_tree_250_files_expands_and_scrolls_within_budget (ui_tests_tabs_tree.rs:194,132,102).
- [~] Project Search — поиск совпадения, переход к строке и пустой результат проверены; include/exclude/filter/help и многократные результаты не проверены: headless_project_search_empty_and_open_result (ui_tests_panels.rs:92; UiId src/ui_system.rs:392-406).
- [x] Git changes — список изменённых файлов появляется без ручного refresh, stage/unstage чекбоксом проверен по `git diff --cached`: headless_bug_git_changes_load_without_manual_refresh (ui_tests_panels.rs), headless_git_file_can_be_staged_and_unstaged (ui_tests_git_commit.rs:41). Git UiId — src/ui_system/ui_ids.rs.
- [x] Git graph и commit history — merge-история отрисовывается с hitbox строк, hover/клик по commit показывает карточку и копирует полный hash, длинная история прокручивается: ui_tests_git_graph.rs:70,89,111 (1280×720 @ 4/3). Отдельного состояния «выбранный commit» нет — клик по GitGraphCommit только перерисовывает (ui_handlers/ui_git.rs).
- [x] Git diff tabs/hunks — открытие diff двойным кликом по файлу, навигация вперёд/назад по трём hunk, откат одного hunk с сохранением (остальные изменения на диске целы), закрытие вкладки; hitbox rollback внутри окна после навигации (был баг: гаттер регистрировал hitbox строк overscan-запаса, исправлено отсечением по зоне редактора в root_frame_renderer.rs): ui_tests_git_diff.rs:83-157. Откат остальных hunk и циклический переход не покрыты.
- [x] Git commit/push flow — commit только staged-файла с введённым сообщением, пункты меню commit и skip hooks, отказ при пустом сообщении и скрытый commit без staged-файлов, push в локальный bare remote: ui_tests_git_commit.rs:59-128. Подтверждения push в UI нет (GitPush пушит сразу); Amend и Commit & Push не выполняются.
- [ ] API Client — только слот/наличие контролов; импорт OpenAPI, auth, параметры, запрос, response/cURL и multipart не проверяются: UiId src/ui_system.rs:93-180; PROJECT_GUIDE.md §2, API Client/API Mock.
- [~] API Mock — включение сервера показывает URL, guide открывается/закрывается и прокручивается; routes/contracts/Python controls и mock request flow не проверяются: headless_database_dialog_and_api_mock_guide_controls, headless_bug_api_mock_guide_wheel_scrolls_content (ui_tests_panels.rs:134,281; UiId src/ui_system.rs:180-235).
- [~] Database panel — при наличии Database Add показывает поля формы, Escape закрывает; соединение, каталог, контекст, таблица, SQL query и DDL не проходят: headless_database_dialog_and_api_mock_guide_controls (ui_tests_panels.rs:134; Database UiId src/ui_system.rs:236-330).
- [~] Terminal — панель открывается, вывод прокручивается и возврат вниз проверен; тест возврата пропускает сценарий при ошибке запуска shell, а поиск/выделение/вкладки терминала не покрыты: headless_terminal_scroll_returns_to_bottom_after_output, headless_bug_terminal_sidebar_slot_opens_panel (ui_tests_panels.rs:227,259; UiId src/ui_system.rs:434-463).
- [~] LSP Servers — для доступного сервера проверены toggle и Stop, также проверен hitbox слота в компактных окнах; тест toggle условно завершает работу без проверки при отсутствии сервера: headless_lsp_server_toggle_exposes_stop_control_when_available, headless_bug_sidebar_slots_hit_lsp_servers_at_small_sizes (ui_tests_panels.rs:159,317).
- [~] Problems panel — слот и набор контролов видимы, нижний resize hitbox остаётся зарегистрирован; список диагностик, jump/copy/URL actions не проверены: headless_ide_panel_slots_open_and_register_controls, headless_ide_panel_splitters_resize_side_and_bottom_panels (ui_tests_panels.rs:35,201; UiId src/ui_system.rs:434-450).
- [~] Splitters и status bar — ширина левой панели меняется, status bar остаётся в пределах окна; изменение высоты нижней панели отдельным assertion не подтверждено: headless_ide_panel_splitters_resize_side_and_bottom_panels, headless_bug_status_bar_stays_inside_with_bottom_panel (ui_tests_panels.rs:201,197).

### Редактор, клавиатура и вкладки

- [~] Клавиатурная маршрутизация — headless UI использует F1, Ctrl+F, Ctrl+Space, Escape, Enter, PageDown, Shift+Right, Backspace, Ctrl+4 и Ctrl+Q; panel shortcuts отдельно не проверяются: ui_tests_editor.rs:10-220, tests.rs:332-408; обработчики editor_keys.rs:631-1013, main_keys.rs:671-740.
- [~] Editor shortcuts — Save (файл на диске, dirty снят), Undo с пустой историей, Undo, Redo по Ctrl+Y, Copy→Paste, Cut→Paste, Paste при пустом буфере, многострочный Copy→Paste и comment toggle на пустой, одной и нескольких строках проверены при 1280×720 @ 4/3: ui_tests_editor.rs:268,302,326,345,365,381. Ctrl+Shift+Z не привязан к Redo.
- [~] Выделение и курсор — drag, Shift+Right и Backspace меняют selection/cursor; расширенные формы выделения не покрыты: headless_editor_selection_and_cursor_at_multiple_scales, headless_editor_drag_selection_does_not_require_double_click (ui_tests_editor.rs:10,145).
- [ ] Multi-cursor — headless UI тестов нет; вывод: поиск по editor.rs, editor_navigation.rs и keyboard-модулям не нашёл multi-cursor символов; наличие реализации другими средствами не проверял.
- [~] Folding — внешняя функция скрывает вложенную fold arrow и раскрывает её обратно; верхний край проверен только на отсутствие ложной стрелки: headless_editor_drag_selection_and_fold_unfold, headless_bug_fold_arrow_top_visible (ui_tests_editor.rs:30,190).
- [~] Minimap — клик меняет scroll_y на двух scale; drag/thumb геометрия и точность позиционирования не проверены: headless_editor_minimap_and_long_scroll_record_converge (ui_tests_editor.rs:73).
- [x] Sticky lines — нет у верха файла, при прокрутке появляется внешний блок, вложенные соседи сменяют друг друга, клик по sticky переводит курсор на заголовок; hitbox с целым y внутри окна при 1280×720 @ 4/3: ui_tests_editor_sticky.rs.
- [~] Markdown preview/Read mode — кликом MarkdownModeToggle включается режим; рендер markdown, scroll, code-copy и scrollbar не проверяются: headless_dump_ui_rect_click_and_settings_key (tests.rs:332; UiId src/ui_system.rs:424-430).
- [ ] Goto definition — headless UI сценария перехода к определению нет: клавиатурные/LSP actions находятся в src/app/keyboard/* и src/app/lsp_actions.rs.
- [x] Completion (Tree-sitter путь, без LSP) — список по префиксу и фильтрация вводом, Escape без вставки, стрелки, Enter/Tab применяют с курсором после вставки, клик: первый выбирает (виден docstring), второй применяет — так задумано; popup у правого/нижнего края внутри окна (был баг, исправлен в render_view/ui.rs autocomplete_popup_x): ui_tests_editor_completion.rs. LSP-completion и сниппеты не покрыты.
- [~] Вкладки: open/close — проверены счётчик, закрытие активной/неактивной вкладки и dirty confirm; drag и полоса вкладок — следующий пункт: headless_tabs_open_counts_and_active_close_switches_to_previous, headless_tabs_closing_inactive_tab_preserves_active_tab, headless_tabs_dirty_close_dialog_discard_closes_tab (ui_tests_tabs_tree.rs).
- [~] Перетаскивание вкладок и прокрутка длинной полосы вкладок — drag активной вкладки меняет порядок и сохраняет содержимое, края ограничивают, клик без движения не переставляет, autoscroll у края при drag, колесо прокручивает полосу с ограничением на обоих концах, новая вкладка попадает в кадр, hitbox вкладок внутри окна с целым y при 1280×720 и 2560×1440 @ 4/3: ui_tests_tabs_tree.rs:396-537. Кнопок прокрутки у полосы нет (только edge fades). Drag неактивной вкладки активирует её и меняет порядок: headless_bug_dragging_inactive_editor_tab_reorders_and_activates_it (ui_tests_tabs_tree.rs:576). Ignored: нет переключения вкладок с клавиатуры Ctrl+Tab — фича (kanri xi36fantzstotb70ffk8rbi8): ui_tests_tabs_tree.rs:607.

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
