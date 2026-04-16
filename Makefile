MAKEFLAGS += --no-print-directory

# Имя бинарника
BINARY_NAME = rriter

# Базовые флаги оптимизации под 5700X3D + Быстрый линковщик LLD
COMMON_RUSTFLAGS = -C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld
BUILD_STD = -Z build-std=core,alloc,std,panic_abort
TARGET = x86_64-unknown-linux-gnu

# Настройки для быстрой сборки (DEBUG=2 дает трейсбеки, PANIC=abort работает с RUST_BACKTRACE)
FAST_PROFILE_OPTS = CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_INCREMENTAL=true CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=2 CARGO_PROFILE_RELEASE_PANIC=abort

# Ультимативные флаги (Fat LTO, v0 mangling, Identical Code Folding, Linker O3)
MAX_RUSTFLAGS = $(COMMON_RUSTFLAGS) -C lto=fat -C symbol-mangling-version=v0 -C link-arg=-Wl,--icf=all -C link-arg=-Wl,-O3

.PHONY: all fast max test clean

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
test:
	@echo "🧪 Запуск тестов (на базе FAST профиля, без пересборки)..."
	$(FAST_PROFILE_OPTS) \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	RUST_BACKTRACE=full \
	cargo +nightly test \
	$(BUILD_STD) \
	--target $(TARGET) \
	--release
	@echo "✅ Тесты завершены"

clean:
	@echo "🧹 Очистка..."
	cargo clean

tree:
	@tree --filelimit 25 -I 'target' || true

api-tree:
	@echo "Обновляю карту проекта..."
	@python3 scripts/make_tree.py
	@echo "✅ Файл PROJECT_MAP.md готов. (Размер: $$(du -h PROJECT_MAP.md | cut -f1))"

actualize:
	cargo +nightly fmt
	make api-tree