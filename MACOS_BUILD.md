# Building and packaging RRiter on macOS

RRiter supports native Apple Silicon, native Intel, and Universal 2 application
bundles. macOS uses AppKit through winit, Retina scaling, an OpenGL 4.1 Core
context, native dialogs/Finder integration, Keychain-backed credentials, native
proxy/trust information, zsh terminal fallback, and managed process groups.

## 1. Prerequisites

Install Xcode Command Line Tools:

```bash
xcode-select --install
```

Install Rust through rustup and select nightly:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain nightly
source "$HOME/.cargo/env"
rustup default nightly
```

Python 3 is supplied by current Xcode Command Line Tools. Verify:

```bash
python3 --version
cargo +nightly -V
xcrun --find clang
codesign --version
```


## 2. Runtime tools

Open **Настройки → Внешние инструменты** and press **Установить** next to
`uv`, `Ruff`, or `Ty`. RRiter uses Astral's official standalone uv installer,
keeps uv, isolated Ruff/Ty environments, and any Python runtime downloaded by
uv below the user's RRiter Application Support/cache directories, leaves shell
profiles and `PATH` unchanged, streams a live log, and supports cancellation.
Installing Ruff or Ty bootstraps uv first when needed. Every update is built in
a fresh generation and becomes active only after its executable passes a version
check; cancelling or failing leaves the previously configured version untouched.

Manual Homebrew or standalone installations are still detected through settings,
`RRITER_*_PATH`, or `PATH` and can be selected explicitly.

## 3. Native build, tests, bundle, DMG, and launch

```bash
python3 scripts/build_macos.py --arch native --install-targets --test --run
```

The default local build is ad-hoc signed and uses hardened runtime. Artifacts are
written under:

```text
dist/macos/RRiter-<version>-<target>.app
dist/macos/RRiter-<version>-<target>.dmg
```

A debug app without DMG:

```bash
python3 scripts/build_macos.py --arch native --debug --no-dmg --run
```

## 4. Universal 2 build

```bash
python3 scripts/build_macos.py --arch universal --install-targets --test
```

The script builds both `aarch64-apple-darwin` and `x86_64-apple-darwin`, combines
them with `lipo`, creates a normal `.app`, signs nested code before the bundle,
and creates a compressed DMG.

## 5. Developer ID signing and notarization

Create a notarytool keychain profile once:

```bash
xcrun notarytool store-credentials RRiterNotary \
  --apple-id '<APPLE_ID>' \
  --team-id '<TEAM_ID>' \
  --password '<APP_SPECIFIC_PASSWORD>'
```

Build, sign, submit, wait, staple, validate, and create the DMG:

```bash
python3 scripts/build_macos.py \
  --arch universal \
  --install-targets \
  --test \
  --sign-identity 'Developer ID Application: Example Name (TEAMID)' \
  --notary-profile RRiterNotary
```

The script verifies the code signature, runs Gatekeeper assessment, submits the
application ZIP and DMG through `notarytool`, and validates stapled tickets.

## 6. Diagnostics

Packaging self-test, available on any OS:

```bash
python3 scripts/build_macos.py --self-test
```

Inspect a produced app:

```bash
codesign --verify --deep --strict --verbose=2 dist/macos/*.app
spctl --assess --type execute --verbose=4 dist/macos/*.app
plutil -p dist/macos/*.app/Contents/Info.plist
lipo -info dist/macos/*.app/Contents/MacOS/RRiter
```

At runtime RRiter prints the selected OpenGL context and detected GPU details.
macOS deliberately has no GLES or legacy OpenGL fallback: it requires the native
OpenGL 4.1 Core profile available on supported macOS systems.
