MAKEFLAGS += --no-print-directory

# Имя бинарника
BINARY_NAME = rriter

# Базовые флаги оптимизации под 5700X3D + Быстрый линковщик LLD
COMMON_RUSTFLAGS = -C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld
BUILD_STD = -Z build-std=core,alloc,std,panic_abort
TARGET = x86_64-unknown-linux-gnu

# Настройки для быстрой сборки (DEBUG=2 дает трейсбеки, PANIC=abort работает с RUST_BACKTRACE)
FAST_PROFILE_OPTS = RUST_BACKTRACE=1 CARGO_BUILD_JOBS=4 CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_INCREMENTAL=true CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_PROFILE_RELEASE_PANIC=abort

# Ультимативные флаги (Fat LTO, v0 mangling, Identical Code Folding, Linker O3)
MAX_RUSTFLAGS = $(COMMON_RUSTFLAGS) -C lto=fat -C symbol-mangling-version=v0 -C link-arg=-Wl,--icf=all -C link-arg=-Wl,-O3

# Настройки тестов
TEST_FILTER ?=
TEST ?=
TEST_THREADS ?= 1
BUILD_STD_TEST = -Z build-std=core,alloc,std,panic_abort,test

.PHONY: all fast max test test-one test-list test-hunt test-time clean

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
	@target/x86_64-unknown-linux-gnu/release/$(BINARY_NAME)

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

PROF_DIR = $(CURDIR)/target/pgo-profiles

pgo-gen:
	@echo "🧬 Сборка для ручного PGO..."
	rm -rf $(PROF_DIR)
	$(FAST_PROFILE_OPTS) \
	RUSTFLAGS="$(COMMON_RUSTFLAGS) -Cprofile-generate=$(PROF_DIR)" \
	cargo +nightly build $(BUILD_STD) --target $(TARGET) --release

pgo-run:
	@echo "🏃 Открываю редактор для профилирования."
	@echo "💾 Создаю бекап ~/.config/RRiter..."
	@cp -r ~/.config/RRiter ~/.config/RRiter_pgo_backup || true
	@echo "🔧 Включаю телеметрию для PGO..."
	@sed -i 's/"enable_telemetry": false/"enable_telemetry": true/' ~/.config/RRiter/config.json || true
	@echo "ВАЖНО: Поскролльте, попечатайте, откройте терминал и ЗАКРОЙТЕ редактор."
	LLVM_PROFILE_FILE="$(PROF_DIR)/default_%p.profraw" target/$(TARGET)/release/$(BINARY_NAME) src/render_view.rs
	@echo "♻️ Восстанавливаю бекап ~/.config/RRiter..."
	@rm -rf ~/.config/RRiter
	@mv ~/.config/RRiter_pgo_backup ~/.config/RRiter || true

pgo-merge:
	@echo "🔗 Слияние профилей..."
	rustup run nightly llvm-profdata merge -o $(PROF_DIR)/merged.profdata $(PROF_DIR)/*.profraw

pgo-max:
	@echo "🔥 Сборка MAX с профилем PGO..."
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
	RUSTFLAGS="$(MAX_RUSTFLAGS) -Cprofile-use=$(PROF_DIR)/merged.profdata" \
	cargo +nightly build $(BUILD_STD) --target $(TARGET) --release

pgo-clean:
	@echo "🧹 Удаление временных файлов PGO..."
	rm -rf $(PROF_DIR)

pgo: pgo-gen pgo-run pgo-merge pgo-max pgo-clean

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