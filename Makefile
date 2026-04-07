# Имя бинарника
BINARY_NAME = rriter

# Базовые флаги оптимизации под 5700X3D
COMMON_RUSTFLAGS = -C target-cpu=native -C llvm-args=-fp-contract=fast
BUILD_STD = -Z build-std=core,alloc,std,panic_abort
TARGET = x86_64-unknown-linux-gnu

# Настройки для быстрой сборки и тестов (идентичные, чтобы не было пересборки)
FAST_PROFILE_OPTS = CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4

.PHONY: all fast max test clean

all: max

# 1. Версия FAST (Сбалансированная)
# Используем для разработки: быстрее сборка, хорошая производительность
fast:
	@echo "🚀 Сборка быстрой версии (Thin LTO, CGU=4)..."
	$(FAST_PROFILE_OPTS) \
	RUSTFLAGS="$(COMMON_RUSTFLAGS)" \
	cargo +nightly build \
	$(BUILD_STD) \
	--target $(TARGET) \
	--release
	@echo "✅ Собрано: target/$(TARGET)/release/$(BINARY_NAME)"

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