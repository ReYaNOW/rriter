# Полная сборка, установка и выпуск RRiter на macOS

RRiter поддерживает актуальные версии macOS на Apple Silicon и Intel, нативные
сборки и Universal 2. Приложение использует AppKit через `winit`, Retina,
OpenGL 4.1 Core, нативные диалоги и Finder, Keychain для секретов, системные
сертификаты и proxy, терминал на основе PTY и управляемые группы дочерних
процессов.

Скрипт `scripts/build_macos.py` создаёт настоящий `.app`, подписывает его,
при необходимости отправляет на notarization и формирует DMG.

## 1. Узнать архитектуру Mac

```bash
uname -m
```

Результат:

- `arm64` — Apple Silicon;
- `x86_64` — Intel.

## 2. Установить инструменты Apple

Для локальной сборки установите Xcode Command Line Tools:

```bash
xcode-select --install
```

Проверьте установку:

```bash
xcode-select -p
xcrun --find clang
xcrun --find codesign
```

Для подписанного публичного выпуска и notarization рекомендуется установить
актуальный Xcode из App Store или Apple Developer, затем выбрать его:

```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
xcodebuild -version
xcrun notarytool --version
```

## 3. Установить Python 3

Не рассчитывайте на наличие Python в Command Line Tools: на чистой современной
macOS установите его отдельно. Например, через Homebrew:

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

После установки добавьте Homebrew в текущую оболочку.

Apple Silicon:

```bash
eval "$(/opt/homebrew/bin/brew shellenv)"
```

Intel:

```bash
eval "$(/usr/local/bin/brew shellenv)"
```

Установите Python:

```bash
brew install python
python3 --version
```

Вместо Homebrew подходит официальный установщик Python с `python.org`, если
команда `python3` после установки доступна в терминале.

## 4. Установить Rust nightly

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain nightly

source "$HOME/.cargo/env"
rustup default nightly
```

Проверка:

```bash
rustc +nightly -Vv
cargo +nightly -V
rustup show
```

## 5. Подготовить проект

При использовании Git:

```bash
mkdir -p ~/projects
cd ~/projects
git clone <RRITER_REPOSITORY_URL> rriter
cd rriter
```

При использовании ZIP распакуйте его и перейдите именно в каталог, где лежат:

```text
Cargo.toml
Cargo.lock
AGENTS.md
scripts/build_macos.py
```

Проверьте скрипт без сборки:

```bash
python3 scripts/build_macos.py --self-test
```

Ожидается:

```text
[rriter-macos] self-test passed
```

## 6. Нативная сборка, тесты, DMG и запуск

```bash
python3 scripts/build_macos.py \
  --arch native \
  --install-targets \
  --test \
  --run
```

Скрипт:

1. определяет архитектуру текущего Mac;
2. устанавливает отсутствующий Rust target, если передан `--install-targets`;
3. запускает тесты только для нативной архитектуры и строго последовательно;
4. собирает RRiter;
5. создаёт `.app` с Retina metadata и иконкой;
6. выполняет локальную ad-hoc подпись с hardened runtime;
7. создаёт сжатый DMG;
8. запускает приложение при наличии `--run`.

Артефакты находятся в:

```text
dist/macos/RRiter-<version>-<target>.app
dist/macos/RRiter-<version>-<target>.dmg
```

Быстрая debug-сборка без DMG:

```bash
python3 scripts/build_macos.py \
  --arch native \
  --debug \
  --no-dmg \
  --run
```

## 7. Минимальная поддерживаемая версия macOS

По умолчанию используется macOS 12.0. Значение применяется одновременно:

- к `MACOSX_DEPLOYMENT_TARGET` при компиляции и линковке Rust;
- к `LSMinimumSystemVersion` внутри `Info.plist`.

Пример сборки для macOS 13.0 и новее:

```bash
python3 scripts/build_macos.py \
  --arch native \
  --minimum-system 13.0 \
  --install-targets \
  --test
```

Нельзя просто уменьшить версию в `Info.plist`: бинарник также должен быть
скомпилирован с тем же deployment target. Скрипт делает это автоматически.

## 8. Universal 2

На Apple Silicon или Intel:

```bash
python3 scripts/build_macos.py \
  --arch universal \
  --install-targets \
  --test
```

Скрипт собирает:

```text
aarch64-apple-darwin
x86_64-apple-darwin
```

Затем объединяет бинарники через `lipo`. Тесты выполняются один раз для
нативной архитектуры текущего Mac: запускать чужую архитектуру через Rosetta
для обычного test run не требуется. Обе архитектуры всё равно отдельно
компилируются в составе Universal 2.

Проверка готового бинарника:

```bash
lipo -archs dist/macos/*.app/Contents/MacOS/RRiter
```

Ожидаются обе архитектуры:

```text
x86_64 arm64
```

## 9. Внешние инструменты RRiter

После запуска откройте:

```text
Настройки → Внешние инструменты
```

Возле `uv`, Ruff и Ty нажмите **Установить**. RRiter:

- использует официальный standalone installer `uv`;
- не изменяет shell profile и глобальный `PATH`;
- хранит управляемые версии в Application Support и cache RRiter;
- изолирует Ruff, Ty и загруженный через `uv` Python;
- показывает журнал и прогресс;
- поддерживает отмену;
- переключается на новое поколение только после успешной проверки версии.

Системные установки и вручную выбранные executable также поддерживаются.

## 10. Подпись Developer ID

Для распространения вне собственного Mac нужен сертификат типа:

```text
Developer ID Application
```

Проверьте доступные identities:

```bash
security find-identity -v -p codesigning
```

Пример подписанной сборки без отправки в Apple:

```bash
python3 scripts/build_macos.py \
  --arch universal \
  --install-targets \
  --test \
  --sign-identity 'Developer ID Application: Example Name (TEAMID)'
```

Скрипт подписывает внутренний executable первым, затем весь `.app`. Для подписи
не применяется `codesign --deep`, чтобы не перезаписывать вложенные подписи.

## 11. Notarization и stapling

Один раз сохраните данные для `notarytool` в Keychain:

```bash
xcrun notarytool store-credentials RRiterNotary \
  --apple-id '<APPLE_ID>' \
  --team-id '<TEAM_ID>' \
  --password '<APP_SPECIFIC_PASSWORD>'
```

Полный выпуск:

```bash
python3 scripts/build_macos.py \
  --arch universal \
  --install-targets \
  --test \
  --sign-identity 'Developer ID Application: Example Name (TEAMID)' \
  --notary-profile RRiterNotary
```

Последовательность:

1. hardened runtime;
2. подпись executable;
3. подпись `.app`;
4. проверка подписи;
5. создание ZIP для notarization;
6. `notarytool submit --wait`;
7. `stapler staple` и `stapler validate` для `.app`;
8. создание DMG;
9. notarization и stapling DMG;
10. обязательная проверка Gatekeeper;
11. вывод SHA-256 готового DMG.

Notarization намеренно запрещена с ad-hoc identity или с
`--no-hardened-runtime`.

## 12. Ручная проверка артефакта

```bash
codesign --verify --deep --strict --verbose=2 dist/macos/*.app
codesign --display --verbose=4 dist/macos/*.app
spctl --assess --type execute --verbose=4 dist/macos/*.app
plutil -p dist/macos/*.app/Contents/Info.plist
lipo -info dist/macos/*.app/Contents/MacOS/RRiter
xcrun stapler validate dist/macos/*.app
xcrun stapler validate dist/macos/*.dmg
```

Проверка запуска из Finder:

```bash
open dist/macos/*.app
```

Для финальной проверки скопируйте приложение из DMG в `/Applications`,
запустите его из Finder и проверьте Gatekeeper без подключённого интернета.

## 13. Типовые ошибки

### `Python 3 is required`

```bash
brew install python
python3 --version
```

### Rust target отсутствует

```bash
rustup target add aarch64-apple-darwin --toolchain nightly
rustup target add x86_64-apple-darwin --toolchain nightly
```

Либо повторите сборку с `--install-targets`.

### Выбран неправильный Xcode

```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
xcode-select -p
xcodebuild -version
```

### `notarytool` отсутствует

Установите актуальный Xcode и повторно выберите его через `xcode-select`.

### Gatekeeper отклоняет локальную ad-hoc сборку

Это ожидаемо для локального непубличного артефакта. Для распространения нужна
Developer ID подпись, hardened runtime, успешная notarization и stapling.

### Приложение не запускается на старой macOS

Проверьте, что `--minimum-system` был задан до сборки, а не только изменён в
`Info.plist`. Скрипт автоматически передаёт это значение Cargo через
`MACOSX_DEPLOYMENT_TARGET`.

## 14. Особенности графики macOS

RRiter на macOS намеренно использует только:

```text
OpenGL 4.1 Core
```

GLES и legacy OpenGL fallback не используются. Retina scale, изменение DPI,
пересоздание font/icon atlases, Command/Option shortcuts, Keychain, Finder,
нативные диалоги и завершение дочерних process groups обслуживаются macOS
backend проекта.
