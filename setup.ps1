# 🚨 核心修复 1：强制锁定工作目录为脚本所在目录，防止路径漂移
Set-Location $PSScriptRoot

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  LuminaConvert - Windows 终极构建脚本" -ForegroundColor Cyan
Write-Host "============================================"
Write-Host ""

Write-Host "[1] 检查 Rust..." -ForegroundColor Yellow
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Host "未安装 Rust" -ForegroundColor Red
    Write-Host "请访问 https://rustup.rs 安装后重试"
    Read-Host "按回车退出"
    exit 1
}
Write-Host "Rust OK" -ForegroundColor Green

Write-Host "[2] 检查 Node.js..." -ForegroundColor Yellow
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "未安装 Node.js" -ForegroundColor Red
    Write-Host "请访问 https://nodejs.org 安装后重试"
    Read-Host "按回车退出"
    exit 1
}
Write-Host "Node.js OK" -ForegroundColor Green

Write-Host "[3] 准备 pnpm 环境..." -ForegroundColor Yellow
# 🚨 核心修复 2：不依赖全局环境变量，使用 npx 临时调用，100% 避免“找不到命令”的报错
$pnpmCmd = "npx pnpm"
if (Get-Command pnpm -ErrorAction SilentlyContinue) {
    $pnpmCmd = "pnpm"
}

Write-Host "[4] 安装依赖..." -ForegroundColor Yellow
Invoke-Expression "$pnpmCmd install"
if ($LASTEXITCODE -ne 0) {
    Write-Host "安装依赖失败" -ForegroundColor Red
    Read-Host "按回车退出"
    exit 1
}

Write-Host "[5] 开始构建 (首次构建可能需要 3-5 分钟)..." -ForegroundColor Yellow
Invoke-Expression "$pnpmCmd tauri build"
if ($LASTEXITCODE -ne 0) {
    Write-Host "构建失败 (请检查是否安装了 Visual Studio C++ Build Tools)" -ForegroundColor Red
    Read-Host "按回车退出"
    exit 1
}

Write-Host "[6] 提取安装包到桌面..." -ForegroundColor Yellow
$desktop = [Environment]::GetFolderPath("Desktop")
$bundleDir = "src-tauri\target\release\bundle"

# 🚨 核心修复 3：使用通配符自动寻找 MSI/EXE 安装包，无视具体文件名
$installers = Get-ChildItem -Path $bundleDir -Include "*.exe", "*.msi" -Recurse -ErrorAction SilentlyContinue
if ($installers) {
    foreach ($file in $installers) {
        Copy-Item $file.FullName "$desktop\" -Force
        Write-Host "已复制: $($file.Name) 到桌面" -ForegroundColor Green
    }
} else {
    Write-Host "未找到安装包，请手动前往 $bundleDir 查找" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "===== 构建完成！按回车退出 =====" -ForegroundColor Cyan
Read-Host