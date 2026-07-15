MAKEFLAGS += --no-print-directory

# Имя бинарника
BINARY_NAME = rriter

# Базовые флаги оптимизации под 5700X3D + Быстрый линковщик LLD
COMMON_RUSTFLAGS = -C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld
BUILD_STD = -Z build-std=core,alloc,std,panic_abort,test
TARGET = x86_64-unknown-linux-gnu
CODEX_ENV = HOME=/home/reyan RUSTUP_HOME=/home/reyan/.local/share/rustup CARGO_HOME=/home/reyan/.local/share/cargo

# Настройки для быстрой сборки (DEBUG=2 дает трейсбеки, PANIC=abort работает с RUST_BACKTRACE)
FAST_PROFILE_OPTS = CARGO_BUILD_JOBS=4 CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_INCREMENTAL=true CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_PROFILE_RELEASE_PANIC=abort

# Ультимативные флаги (Fat LTO, v0 mangling, Identical Code Folding, Linker O3)
MAX_RUSTFLAGS = $(COMMON_RUSTFLAGS) -C lto=fat -C symbol-mangling-version=v0 -C link-arg=-Wl,--icf=all -C link-arg=-Wl,-O3

# Настройки тестов
TEST_FILTER ?=
TEST ?=
TEST_THREADS ?= 1
BUILD_STD_TEST = $(BUILD_STD)

.PHONY: all fast max bloat-max codex_test test test-one test-list test-hunt test-time scroll-bench pgo-bench-tools pgo-bench-self-test pgo-bench-build pgo-bench-run pgo-bench pgo-gen pgo-run pgo-merge pgo-max pgo-auto pgo-gen-fast pgo-script pgo-train pgo-use pgo-clean pgo clean

all: max

# 1. Версия FAST (Сбалансированная)
# Используем для разработки: быстрее сборка, хорошая производительность
fast:
	@echo "🚀 Сборка быстрой версии (Incremental, No Strip, LLD)..."
	@$(FAST_PROFILE_OPTS) \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	cargo +nightly build \
	$(BUILD_STD) \
	--target $(TARGET) \
	--release
	@echo "✅ Собрано: target/$(TARGET)/release/$(BINARY_NAME)"

run:
	@$(MAKE) fast
	@RUST_BACKTRACE=full target/x86_64-unknown-linux-gnu/release/$(BINARY_NAME)

scroll-bench:
	@$(MAKE) fast
	@rustc --edition=2021 -O tests/perf_scroll_motion.rs -o /tmp/rriter_scroll_bench
	@/tmp/rriter_scroll_bench tests/perf_large_realistic_15000.py 22

# 2. Версия MAX (Ультимативная)
# Для финального использования. Медленная сборка, максимальный FPS
max:
	@echo "🔥 Сборка ультимативной версии (Fat LTO, Immediate Abort)..."
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
	RUSTFLAGS="$(MAX_RUSTFLAGS)" \
	cargo +nightly build \
	$(BUILD_STD) \
	--target $(TARGET) \
	--release
	@echo "✅ Собрано: target/$(TARGET)/release/$(BINARY_NAME)"

bloat-max:
	@echo "📦 Анализ размера MAX-сборки..."
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
	RUSTFLAGS="$(MAX_RUSTFLAGS)" \
	cargo +nightly bloat \
	$(BUILD_STD) \
	--target $(TARGET) \
	--release \
	--bin $(BINARY_NAME) \
	--crates \
	-n 40

codex_test:
	@$(CODEX_ENV) $(MAKE) test
	@$(CODEX_ENV) $(MAKE) fast
	
# 3. Команда TEST
# Главное: флаги идентичны команде 'fast', поэтому пересборки не будет.
# RUST_BACKTRACE=full для детального отчета при ошибках.
# --nocapture показывает stdout/stderr тестов сразу.
# --test-threads=1 делает вывод последовательным, чтобы было видно, где зависло.
test:
	@echo "🧪 Запуск тестов (на базе FAST профиля, подробный режим)..."
	$(FAST_PROFILE_OPTS) \
	CARGO_TERM_COLOR=always \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	RUST_BACKTRACE=full \
	cargo +nightly test \
	$(BUILD_STD_TEST) \
	-Z panic-abort-tests \
	--target $(TARGET) \
	--release \
	$(TEST_FILTER) \
	-- \
	--color always \
	--nocapture \
	--test-threads=$(TEST_THREADS)
	@echo "✅ Тесты завершены"

test-cov:
	cargo +nightly llvm-cov --summary-only

test-cov-full:
	cargo +nightly llvm-cov --show-missing-lines --summary-only

# Запуск одного конкретного теста:
# make test-one TEST='module::test_name'
test-one:
	@if [ -z "$(TEST)" ]; then \
		echo "❌ Укажи тест: make test-one TEST='module::test_name'"; \
		exit 2; \
	fi
	@echo "🎯 Запуск одного теста: $(TEST)"
	$(FAST_PROFILE_OPTS) \
	CARGO_TERM_COLOR=always \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	RUST_BACKTRACE=full \
	cargo +nightly test \
	$(BUILD_STD_TEST) \
	-Z panic-abort-tests \
	--target $(TARGET) \
	--release \
	-- \
	--color always \
	--nocapture \
	--test-threads=1 \
	--exact "$(TEST)"

# Список всех тестов
test-list:
	@echo "📋 Список тестов..."
	$(FAST_PROFILE_OPTS) \
	CARGO_TERM_COLOR=always \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	RUST_BACKTRACE=full \
	cargo +nightly test \
	$(BUILD_STD_TEST) \
	-Z panic-abort-tests \
	--target $(TARGET) \
	--release \
	-- \
	--list


# Тесты с таймингами.
test-time:
	@echo "⏱️ Запуск тестов с таймингами..."
	$(FAST_PROFILE_OPTS) \
	CARGO_TERM_COLOR=always \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	RUST_BACKTRACE=full \
	cargo +nightly test \
	$(BUILD_STD_TEST) \
	-Z panic-abort-tests \
	--target $(TARGET) \
	--release \
	$(TEST_FILTER) \
	-- \
	-Z unstable-options \
	--report-time \
	--color always \
	--nocapture \
	--test-threads=$(TEST_THREADS)
	@echo "✅ Тесты завершены"

PROF_DIR ?= $(CURDIR)/target/pgo-profiles/$(TARGET)
PGO_GENERATE_TARGET_DIR ?= $(CURDIR)/target/pgo-generate/$(TARGET)
PGO_USE_TARGET_DIR ?= $(CURDIR)/target/pgo-use/$(TARGET)
PGO_TRAINING_DIR ?= $(CURDIR)/target/pgo-training/$(TARGET)
PGO_AUTOMATION_TIMEOUT ?= 600
PGO_FAST_EXECUTABLE ?= $(CURDIR)/target/$(TARGET)/release/$(BINARY_NAME)
PGO_COMPARE_DIR ?= $(CURDIR)/target/pgo-compare
PGO_BENCH_TOOL_DIR ?= $(CURDIR)/target/pgo-bench-tools
PGO_BENCH_RUNS ?= 7
PGO_BENCH_WARMUP ?= 2
PGO_SCROLL_RUNS ?= 2
PGO_SCROLL_WARMUP ?= 1
PGO_SCROLL_SECONDS ?= 12
PGO_BENCH_ARGS ?=
PGO_BENCH_BUILD_TOOL = $(PGO_BENCH_TOOL_DIR)/pgo-bench-build
PGO_BENCH_COMPARE_TOOL = $(PGO_BENCH_TOOL_DIR)/pgo-bench-compare

$(PGO_BENCH_BUILD_TOOL): scripts/pgo_bench_build.rs scripts/pgo_bench_compare.rs
	@mkdir -p $(PGO_BENCH_TOOL_DIR)
	rustc --edition=2021 -O -D warnings scripts/pgo_bench_build.rs -o $(PGO_BENCH_BUILD_TOOL)

$(PGO_BENCH_COMPARE_TOOL): scripts/pgo_bench_compare.rs
	@mkdir -p $(PGO_BENCH_TOOL_DIR)
	rustc --edition=2021 -O -D warnings scripts/pgo_bench_compare.rs -o $(PGO_BENCH_COMPARE_TOOL)

pgo-bench-tools: $(PGO_BENCH_BUILD_TOOL) $(PGO_BENCH_COMPARE_TOOL)

pgo-bench-self-test: pgo-bench-tools
	$(PGO_BENCH_BUILD_TOOL) --self-test
	$(PGO_BENCH_COMPARE_TOOL) --self-test

pgo-bench-build: $(PGO_BENCH_BUILD_TOOL)
	$(PGO_BENCH_BUILD_TOOL) \
		--profile $(PROF_DIR)/merged.profdata \
		--out-dir $(PGO_COMPARE_DIR) \
		--target $(TARGET) \
		--build-std \
		--rustflag=-Ctarget-cpu=native \
		--rustflag=-Cllvm-args=-fp-contract=fast \
		--rustflag=-Clto=fat \
		--rustflag=-Csymbol-mangling-version=v0 \
		--rustflag=-Clink-arg=-fuse-ld=lld \
		--rustflag=-Clink-arg=-Wl,--icf=all \
		--rustflag=-Clink-arg=-Wl,-O3 \
		--workspace $(CURDIR) \
		--fixture $(CURDIR)/tests/perf_large_realistic_15000.py \
		--runs $(PGO_BENCH_RUNS) \
		--warmup $(PGO_BENCH_WARMUP) \
		--scroll-runs $(PGO_SCROLL_RUNS) \
		--scroll-warmup $(PGO_SCROLL_WARMUP) \
		--scroll-seconds $(PGO_SCROLL_SECONDS)

pgo-bench-run: $(PGO_BENCH_COMPARE_TOOL)
	$(PGO_BENCH_COMPARE_TOOL) \
		--baseline $(PGO_COMPARE_DIR)/baseline/$(BINARY_NAME) \
		--pgo $(PGO_COMPARE_DIR)/pgo/$(BINARY_NAME) \
		--workspace $(CURDIR) \
		--fixture $(CURDIR)/tests/perf_large_realistic_15000.py \
		--git-repo $(CURDIR) \
		--runs $(PGO_BENCH_RUNS) \
		--warmup $(PGO_BENCH_WARMUP) \
		--scroll-runs $(PGO_SCROLL_RUNS) \
		--scroll-warmup $(PGO_SCROLL_WARMUP) \
		--scroll-seconds $(PGO_SCROLL_SECONDS) \
		--csv $(PGO_COMPARE_DIR)/report.csv $(PGO_BENCH_ARGS)

pgo-bench: pgo-bench-build pgo-bench-run

pgo-gen:
	@echo "🧬 Сборка изолированной версии для ручного PGO..."
	rm -rf $(PROF_DIR)
	@mkdir -p $(PROF_DIR)
	CARGO_TARGET_DIR="$(PGO_GENERATE_TARGET_DIR)" \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=abort \
	RUSTFLAGS="$(MAX_RUSTFLAGS) -Cprofile-generate=$(PROF_DIR)" \
	cargo +nightly build $(BUILD_STD) --target $(TARGET) --release --bin $(BINARY_NAME)
	@echo "✅ Инструментированный RRiter: $(PGO_GENERATE_TARGET_DIR)/$(TARGET)/release/$(BINARY_NAME)"

pgo-run:
	@echo "🏃 Открываю инструментированный RRiter в полноценном IDE-режиме."
	@echo "ВАЖНО: используйте нужные функции и штатно ЗАКРОЙТЕ редактор."
	LLVM_PROFILE_FILE="$(PROF_DIR)/manual_%p_%m.profraw" \
		"$(PGO_GENERATE_TARGET_DIR)/$(TARGET)/release/$(BINARY_NAME)" --ide

pgo-merge:
	@echo "🔗 Слияние профилей..."
	@test -n "$$(find "$(PROF_DIR)" -maxdepth 1 -type f -name '*.profraw' -print -quit)" || \
		(echo "❌ В $(PROF_DIR) нет .profraw; сначала выполните make pgo-run" && exit 2)
	rustup run nightly llvm-profdata merge -sparse \
		-o "$(PROF_DIR)/merged.profdata" "$(PROF_DIR)"/*.profraw
	@echo "✅ Профиль: $(PROF_DIR)/merged.profdata"

pgo-max:
	@echo "🔥 Изолированная MAX-сборка с PGO..."
	@test -s "$(PROF_DIR)/merged.profdata" || \
		(echo "❌ Нет $(PROF_DIR)/merged.profdata; сначала выполните make pgo-merge" && exit 2)
	CARGO_TARGET_DIR="$(PGO_USE_TARGET_DIR)" \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
	RUSTFLAGS="$(MAX_RUSTFLAGS) -Cprofile-use=$(PROF_DIR)/merged.profdata -Cllvm-args=-pgo-warn-missing-function" \
	cargo +nightly build $(BUILD_STD) --target $(TARGET) --release --bin $(BINARY_NAME)
	@echo "✅ PGO RRiter: $(PGO_USE_TARGET_DIR)/$(TARGET)/release/$(BINARY_NAME)"

pgo-auto:
	@echo "🤖 Полный свежий PGO: сборка → GUI-тренировка → merge → MAX-сборка"
	python3 scripts/pgo_pipeline.py \
		--target "$(TARGET)" \
		--mode fresh \
		--build-std \
		--timeout-seconds "$(PGO_AUTOMATION_TIMEOUT)" \
		--rustflags "$(MAX_RUSTFLAGS)" \
		--env CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
		--env CARGO_PROFILE_RELEASE_PANIC=immediate-abort

pgo-gen-fast:
	@echo "⚡ Быстрый RRiter для отладки PGO GUI-сценария (без PGO/MAX/LTO)..."
	@$(MAKE) fast
	@echo "✅ Тестовый бинарник: $(PGO_FAST_EXECUTABLE)"

pgo-script:
	@echo "🎬 Только GUI-автоматизация на быстром RRiter — без сборки, merge и PGO-use"
	python3 scripts/pgo_pipeline.py \
		--target "$(TARGET)" \
		--run-only \
		--run-executable "$(PGO_FAST_EXECUTABLE)" \
		--timeout-seconds "$(PGO_AUTOMATION_TIMEOUT)"

pgo-train:
	@echo "🤖 Создание свежего профиля без финальной PGO-сборки"
	python3 scripts/pgo_pipeline.py \
		--target "$(TARGET)" \
		--mode fresh \
		--train-only \
		--build-std \
		--timeout-seconds "$(PGO_AUTOMATION_TIMEOUT)" \
		--rustflags "$(MAX_RUSTFLAGS)" \
		--env CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
		--env CARGO_PROFILE_RELEASE_PANIC=immediate-abort

pgo-use:
	@echo "♻️ Проверка и использование совместимого PGO-профиля"
	python3 scripts/pgo_pipeline.py \
		--target "$(TARGET)" \
		--mode reuse \
		--build-std \
		--timeout-seconds "$(PGO_AUTOMATION_TIMEOUT)" \
		--rustflags "$(MAX_RUSTFLAGS)" \
		--env CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
		--env CARGO_PROFILE_RELEASE_PANIC=immediate-abort

pgo-clean:
	@echo "🧹 Удаление только изолированных PGO-артефактов..."
	rm -rf "$(PROF_DIR)" "$(PGO_TRAINING_DIR)" \
		"$(PGO_GENERATE_TARGET_DIR)" "$(PGO_USE_TARGET_DIR)"

pgo: pgo-auto

clean:
	@echo "🧹 Очистка..."
	cargo +nightly clean

tree:
	@tree --filelimit 25 -I 'target' || true

api-map:
	@echo "Обновляю карту проекта..."
	@python3 scripts/gen_project_map.py
	@echo "✅ Файл PROJECT_MAP.xml готов"

actualize:
	cargo +nightly fmt
	make api-map
