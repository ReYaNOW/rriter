# RRiter

RRiter is a code editor built with an immediate-mode interface and GPU-centric rendering.
It keeps heavy work asynchronous and emphasizes a responsive input and render path.

## Build and install on Linux

### Prerequisites

The repository does not include a `rust-toolchain` file. The Makefile uses the Rust
`nightly` toolchain, Rust 2024 edition, and `-Z build-std`, so install rustup and
nightly with its Rust sources:

```sh
rustup toolchain install nightly --component rust-src
```

The Makefile also selects the `x86_64-unknown-linux-gnu` target and LLD linker.
Install a C build toolchain, LLD, and the Linux Wayland, EGL, OpenGL, and
xkbcommon runtime/development libraries. For example:

```sh
# Arch Linux
sudo pacman -S --needed base-devel git lld wayland libxkbcommon mesa

# Debian / Ubuntu
sudo apt install build-essential git lld libwayland-dev libxkbcommon-dev libegl-dev libgl-dev
```

The default Linux build enables Wayland and EGL; X11 is an optional Cargo feature
and is not enabled by the Makefile's build target.

### Build

From the repository root, build the release binary with:

```sh
make fast
```

The binary is written to `target/x86_64-unknown-linux-gnu/release/rriter`.
It is built with `-C target-cpu=native`, so build it on the machine that will run it.
Install it in your user bin directory and launch it with:

```sh
mkdir -p ~/.local/bin
cp target/x86_64-unknown-linux-gnu/release/rriter ~/.local/bin/rriter
rriter
```

To update a checkout and rebuild, run `git pull`, then repeat `make fast` and the
copy command above.

## PDF support

On first use of a PDF, if the PDF engine is missing, use the download action in
the PDF tab. RRiter fetches the pinned PDFium `chromium/8066` library into its
managed data directory, normally
`~/.local/share/RRiter/tools/managed/pdfium/chromium-8066/libpdfium.so`.
You can set `RRITER_PDFIUM_PATH` to an existing `libpdfium.so` to use another copy.

To fetch it before opening a PDF, run `make pdfium` from the repository root;
`python3 scripts/fetch_pdfium.py` is the underlying fetch command.

## User files and optional tools

On Linux, RRiter uses the XDG directories and app folder `RRiter`:

- Configuration: `${XDG_CONFIG_HOME:-~/.config}/RRiter`
- Data: `${XDG_DATA_HOME:-~/.local/share}/RRiter`
- Cache: `${XDG_CACHE_HOME:-~/.cache}/RRiter`

Git, `rust-analyzer`, Python, Ruff, Ty, uv, Dart, and a shell are optional tools
used by relevant editor features. RRiter can install uv, Ruff, and Ty from its
settings; unavailable tools are shown as disabled. Tool paths can also be set
in settings or through their `RRITER_*_PATH` environment overrides.
