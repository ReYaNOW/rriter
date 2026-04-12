MAKEFLAGS += --no-print-directory

# Имя бинарника
BINARY_NAME = rriter

# Базовые флаги оптимизации под 5700X3D + Быстрый линковщик LLD
COMMON_RUSTFLAGS = -C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld
BUILD_STD = -Z build-std=core,alloc,std,panic_abort
TARGET = x86_64-unknown-linux-gnu

# Настройки для быстрой сборки: ИНКРЕМЕНТАЛЬНАЯ сборка и без урезания бинарника (strip)
FAST_PROFILE_OPTS = CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_INCREMENTAL=true CARGO_PROFILE_RELEASE_STRIP=none

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
	@echo "🔥 Сборка ультимативной версии (Fat LTO, CGU=1)..."
	CARGO_PROFILE_RELEASE_LTO=fat \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
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