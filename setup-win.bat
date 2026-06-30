@echo off
chcp 65001 >nul
title LuminaConvert 环境部署

:: ---------- 配置 ----------
set GH_PROXY=https://ghproxy.com
set NPM_REGISTRY=https://registry.npmmirror.com
set RUSTUP_MIRROR=https://mirrors.ustc.edu.cn/rust-static

:: ---------- 检测管理员 ----------
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo 请右键 → 以管理员身份运行
    pause & exit /b 1
)

echo ===== LuminaConvert Windows 环境部署 =====
echo.

:: 1. Git
echo [1/5] Git
winget install --id Git.Git -e --source winget --silent --accept-package-agreements >nul 2>&1
if %errorlevel% equ 0 (echo   OK) else (echo   已存在，跳过)
"%ProgramFiles%\Git\bin\git" --version 2>nul && set "PATH=%ProgramFiles%\Git\bin;%PATH%"

:: 2. Node.js 22
echo [2/5] Node.js 22
winget install --id OpenJS.NodeJS.LTS -e --source winget --silent --accept-package-agreements >nul 2>&1
if %errorlevel% equ 0 (echo   OK) else (echo   已存在，跳过)
call npm config set registry %NPM_REGISTRY%

:: 3. Rust
echo [3/5] Rust
set RUSTUP_DIST_SERVER=%RUSTUP_MIRROR%
set RUSTUP_UPDATE_ROOT=%RUSTUP_MIRROR%/rustup
curl -fsSL -o "%TEMP%\rustup-init.exe" "%GH_PROXY%/https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
"%TEMP%\rustup-init.exe" -y --default-toolchain stable --profile default >nul 2>&1
:: cargo 镜像
if not exist "%USERPROFILE%\.cargo\config.toml" (
    echo [source.crates-io] > "%USERPROFILE%\.cargo\config.toml"
    echo replace-with = "ustc" >> "%USERPROFILE%\.cargo\config.toml"
    echo [source.ustc] >> "%USERPROFILE%\.cargo\config.toml"
    echo registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/" >> "%USERPROFILE%\.cargo\config.toml"
)
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
echo   OK

:: 4. libvips all
echo [4/5] libvips all 版 (含 HEIC)
set VIPS_VER=8.18.2
set VIPS_ROOT=C:\vips\vips-dev-8.18
curl -L -o "%TEMP%\vips.zip" "%GH_PROXY%/https://github.com/libvips/build-win64-mxe/releases/download/v%VIPS_VER%/vips-dev-x64-all-%VIPS_VER%.zip"
powershell -Command "Expand-Archive -Path '%TEMP%\vips.zip' -DestinationPath 'C:\vips' -Force"
setx PATH "%VIPS_ROOT%\bin;%PATH%" /M >nul
setx VIPS_INCLUDE_DIR "%VIPS_ROOT%\include" /M >nul
setx VIPS_LIB_DIR "%VIPS_ROOT%\lib" /M >nul
set "PATH=%VIPS_ROOT%\bin;%PATH%"
echo   OK

:: 5. 项目
echo [5/5] 项目依赖
cd /d "%~dp0"
if not exist "src-tauri\resources\vips" (
    xcopy /E /I /Y "C:\vips\vips-dev-8.18" "src-tauri\resources\vips" >nul
)
call npm install
echo   OK

echo.
echo ===== 完成！运行命令：npm run tauri dev =====
pause
