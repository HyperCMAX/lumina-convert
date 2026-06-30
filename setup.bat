@echo off
:: 强制开启 UTF-8 编码，彻底解决中文导致的闪退和乱码
chcp 65001 >nul
:: 强制锁定当前目录为脚本所在目录，防止路径漂移
cd /d "%~dp0"

echo ============================================
echo   LuminaConvert - Windows 终极构建脚本
echo ============================================
echo.

echo [1] 检查 Rust 环境...
where rustc >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未检测到 Rust！
    echo 请访问 https://rustup.rs 下载并运行 rustup-init.exe
    goto :end
)
echo Rust 环境 OK!
echo.

echo [2] 检查 Node.js 环境...
where node >nul 2>nul
if %errorlevel% neq 0 (
    echo [错误] 未检测到 Node.js！
    echo 请访问 https://nodejs.org 下载并安装 LTS 版本
    goto :end
)
echo Node.js 环境 OK!
echo.

echo [3] 准备 pnpm 环境...
where pnpm >nul 2>nul
if %errorlevel% neq 0 (
    echo 未找到 pnpm，正在尝试使用 npm 全局安装...
    call npm install -g pnpm
)
echo.

echo [4] 安装前端依赖...
:: 使用 call 防止 npm/pnpm 执行完后直接带走整个脚本进程
call pnpm install
if %errorlevel% neq 0 (
    echo pnpm 安装失败，尝试降级使用 npm...
    call npm install
    if %errorlevel% neq 0 (
        echo [错误] 依赖安装失败！请检查网络或 node_modules 权限。
        goto :end
    )
)
echo.

echo [5] 开始构建 Tauri 应用 (首次构建需要 3-10 分钟，请耐心等待)...
call pnpm tauri build
if %errorlevel% neq 0 (
    echo.
    echo [错误] Tauri 构建失败！
    echo 常见原因：
    echo 1. 未安装 Visual Studio C++ Build Tools。
    echo 2. 安装 C++ Build Tools 时未勾选 "Windows 10/11 SDK" 和 "C++ 桌面开发"。
    goto :end
)
echo.

echo [6] 提取安装包到桌面...
:: 使用内联 PowerShell 进行智能文件搜索和复制
powershell -Command "$desktop = [Environment]::GetFolderPath('Desktop'); $bundleDir = 'src-tauri\target\release\bundle'; $files = Get-ChildItem -Path $bundleDir -Include '*.exe','*.msi' -Recurse -ErrorAction SilentlyContinue; if($files) { foreach($f in $files) { Copy-Item $f.FullName $desktop -Force; Write-Host ('成功复制: ' + $f.Name + ' 到桌面') -ForegroundColor Green } } else { Write-Host '未找到安装包，请手动前往 src-tauri\target\release\bundle 查找' -ForegroundColor Yellow }"

:end
echo.
echo ============================================
echo 脚本执行结束。请查看上方的输出信息。
echo ============================================
:: 🚨 最核心的防闪退指令：无论成功还是失败，强制暂停等待用户按键
pause