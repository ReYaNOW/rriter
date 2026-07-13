# Building and running RRiter on a clean Windows 11 machine

RRiter targets 64-bit Windows 11 through the MSVC Rust target. The supported
build path uses native Windows tools; WSL, MinGW, Git Bash, and a full Visual
Studio IDE are not required.

## 1. Install prerequisites

Open **PowerShell as a normal user**. `winget` is included with current Windows
11 through App Installer.

```powershell
winget install --id Git.Git -e --source winget
winget install --id Python.Python.3.13 -e --source winget
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --source winget --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

The Visual Studio workload supplies MSVC, `link.exe`, `rc.exe`, and the Windows
SDK. Restart PowerShell after the installers finish.

Install Rust through the official `rustup-init.exe` bootstrapper. RRiter uses a
nightly-only Cargo/profile capability, so the default toolchain must be nightly.

```powershell
Set-ExecutionPolicy -Scope Process Bypass
Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile "$env:TEMP\rustup-init.exe"
& "$env:TEMP\rustup-init.exe" -y --profile minimal --default-toolchain nightly
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustup default nightly
rustup target add x86_64-pc-windows-msvc --toolchain nightly
```

Verify the native toolchain:

```powershell
rustc +nightly -Vv
cargo +nightly -V
py -3 --version
git --version
```

The repository may contain paths longer than the historical Git limit. Enable
Git's long-path support once:

```powershell
git config --global core.longpaths true
```

## 2. Obtain the source

From Git:

```powershell
git clone <RRITER_REPOSITORY_URL> rriter
Set-Location .\rriter
```

Or from a source ZIP:

```powershell
New-Item -ItemType Directory -Force .\rriter | Out-Null
Expand-Archive .\rriter.zip -DestinationPath .\rriter
Set-Location .\rriter
```

`Cargo.toml`, `Cargo.lock`, `AGENTS.md`, and `scripts\build_windows.py` must be
located directly in the current directory. If the ZIP contains one outer
folder, enter that folder before building.

## 3. Build, run tests, package, and launch

The project script discovers Visual Studio Build Tools with `vswhere.exe`,
imports the x64 MSVC environment, embeds the DPI/long-path manifest and native
icon/version resources, runs the Windows test suite, builds release RRiter,
creates a portable ZIP, and launches the executable:

```powershell
py -3 .\scripts\build_windows.py --install-target --test --run
```

Artifacts:

```text
target\x86_64-pc-windows-msvc\release\rriter.exe
dist\windows\RRiter-<version>-x86_64-portable.zip
```

Subsequent build and launch without tests:

```powershell
py -3 .\scripts\build_windows.py --run
```

Fast debug build without packaging:

```powershell
py -3 .\scripts\build_windows.py --debug --no-package --run
```

## 4. Optional installer

Install Inno Setup 6:

```powershell
winget install --id JRSoftware.InnoSetup -e --source winget
```

Build the per-user installer:

```powershell
py -3 .\scripts\build_windows.py --installer
```

Result:

```text
dist\windows\RRiter-<version>-x86_64-setup.exe
```

The installer uses `%LOCALAPPDATA%\Programs\RRiter`, creates Start Menu entries,
registers `rriter.exe` under the current user's App Paths, and preserves user
configuration during upgrades and uninstall.

## 5. Optional code signing

Sign with a PFX file. Keep the password out of command history:

```powershell
$env:RRITER_SIGN_PASSWORD = Read-Host -AsSecureString | ConvertFrom-SecureString -AsPlainText
$env:RRITER_TIMESTAMP_URL = "http://timestamp.digicert.com"
py -3 .\scripts\build_windows.py --installer `
  --sign-pfx .\private\rriter-signing.pfx `
  --sign-password-env RRITER_SIGN_PASSWORD
Remove-Item Env:RRITER_SIGN_PASSWORD
```

A certificate already installed in the Windows certificate store can be
selected by SHA-1 thumbprint:

```powershell
py -3 .\scripts\build_windows.py --installer --sign-cert-sha1 <THUMBPRINT>
```

Both the application executable and installer are verified with `signtool.exe`.

## 6. Runtime tools

RRiter discovers Git, Ruff, Ty, uv, Python, and the preferred terminal shell
through its settings page, explicit `RRITER_*_PATH` environment variables, or
`PATH`, in that order. Missing tools remain disabled without restart spam.

Open **Настройки → Внешние инструменты** and press **Установить** next to
`uv`, `Ruff`, or `Ty`. RRiter downloads the official uv installer, keeps uv,
isolated Ruff/Ty environments, and any Python runtime downloaded by uv under
the current user's RRiter data/cache directories, does not modify `PATH`, the
Windows Python registry, or PowerShell profiles, shows a live log, and supports
cancellation. Installing Ruff or Ty automatically bootstraps uv when it is
missing.

The equivalent manual setup remains optional:

```powershell
winget install --id astral-sh.uv -e --source winget
uv tool install ruff@latest
uv tool install ty@latest
uv tool update-shell
```

PowerShell 7 is optional; RRiter otherwise falls back to Windows PowerShell or
`cmd.exe`:

```powershell
winget install --id Microsoft.PowerShell -e --source winget
```

For manually installed tools, open RRiter's settings and press **Обновить** or
select each executable explicitly. Typical locations include:

```text
%USERPROFILE%\.local\bin\uv.exe
%USERPROFILE%\.local\bin\ruff.exe
%USERPROFILE%\.local\bin\ty.exe
C:\Program Files\Git\cmd\git.exe
C:\Program Files\PowerShell\7\pwsh.exe
```

## 7. Troubleshooting

Run the packaging self-test without compiling Rust:

```powershell
py -3 .\scripts\build_windows.py --self-test
```

If the script cannot find MSVC, confirm the workload is installed, then rerun
from a fresh PowerShell window. The script imports `VsDevCmd.bat` itself; it is
not necessary to use Developer PowerShell.

If `cargo` cannot find `link.exe`, verify:

```powershell
& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" `
  -latest -products * `
  -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
  -property installationPath
```

RRiter logs the selected OpenGL context, GPU vendor, renderer, GL/GLSL versions,
and DPI scale at startup. The same report can be copied from settings. Windows
requires a desktop OpenGL 3.3 or newer driver; RRiter first requests 4.1 Core and
then falls back to 3.3 Core.
