param(
    [string]$Target = "ddddocr.onnx"
)

$ErrorActionPreference = "Stop"

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    Write-Host "未找到 python，请先安装 Python 并执行: pip install ddddocr" -ForegroundColor Red
    exit 1
}

$code = @'
import ddddocr, os
print(os.path.join(os.path.dirname(ddddocr.__file__), "common_old.onnx"))
'@

$modelPath = $code | python -
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $modelPath)) {
    Write-Host "当前 Python 环境没有 ddddocr 包，请先执行: pip install ddddocr" -ForegroundColor Red
    exit 1
}

Copy-Item -LiteralPath $modelPath -Destination $Target -Force
Write-Host "已将 ddddocr 模型复制到 $Target" -ForegroundColor Green
