# Дизайн исправления четырёх багов RRiter

## Требования

1. Невидимый или ещё не отрисованный hover popup не должен перехватывать первый wheel event в Python editor, в случайных hover-состояниях и в Markdown Reader. Wheel должен потребляться popup только внутри реально отрисованной интерактивной поверхности.
2. Переход к результату поиска при вводе и навигации стрелками должен оставлять anchor результата в центральной полосе viewport от 35% до 65% высоты. Если anchor уже внутри полосы, scroll не меняется; иначе выбирается ближайшая граница полосы, чтобы скачок был минимальным.
3. `Ctrl+/` должен синхронно применять те же insert/delete edits к highlighter replica, которые editor сформировал для comment toggle, чтобы tree и backing text никогда не расходились и byte range не мог выйти за границы строки.
4. Настроенный Ctrl+wheel multiplier должен одинаково и ровно один раз применяться к `LineDelta` и `PixelDelta`; значение `5x` должно давать фактический пятикратный input delta.

## Архитектура

- Hover interaction использует сохранённый renderer-owned `interaction_rect`, совпадающий с реально видимым outer frame popup. Отсутствие этого rect означает отсутствие интерактивного popup; очистка hover не завершает обработку wheel event.
- Search вычисляет минимальный target общей allocation-free функцией `search_anchor_central_band_target`. Markdown Edit использует fold-aware visual anchor, Markdown Reader — source-to-layout geometry, pending mode transition — destination coordinates.
- Comment toggle остаётся одной editor history operation; новый keyboard helper проигрывает только вновь созданные `sync_edits` в highlighter replica перед общей отправкой worker edits.
- Wheel normalization выполняется один раз в `wheel_delta` для обоих вариантов `MouseScrollDelta`.

## Ограничения

- Изменения хирургические, без новых зависимостей и без I/O или новых allocations в render/input hot paths.
- Существующие scroll ownership, Markdown transition и editor history semantics сохраняются.
- Все regression tests проверяют пользовательское поведение, включая невидимый hover, folded Markdown, pending transition и PixelDelta.

