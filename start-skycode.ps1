# start-skycode.ps1 - Minimal launcher for SkyCodeOS.
#
# Usage:
#   .\start-skycode.ps1                  # start OpenAI-compatible API server on 11434
#   .\start-skycode.ps1 -Ask "<task>"    # one-shot interactive ask
#   .\start-skycode.ps1 -Mcp             # start MCP stdio server (Claude Desktop / Cursor)
#   .\start-skycode.ps1 -Mcp -Sse        # start MCP SSE server on 11435 (LAN clients)
#   .\start-skycode.ps1 -Port 9000       # API on a custom port
#
# Verifies prerequisites before starting. Prints active profile and endpoint URLs.

param(
    [string]$Ask,
    [switch]$Mcp,
    [switch]$Sse,
    [int]$Port = 11434
)

$ErrorActionPreference = "Stop"

# 1. Anchor to script directory (project root)
$root = $PSScriptRoot
Set-Location $root

# 2. Locate the scos binary - prefer local debug, fall back to ~/.cargo/bin
$scos = Join-Path $root "target\debug\scos.exe"
if (-not (Test-Path $scos)) {
    $scos = Join-Path $env:USERPROFILE ".cargo\bin\scos.exe"
}
if (-not (Test-Path $scos)) {
    Write-Host "scos.exe not found." -ForegroundColor Red
    Write-Host "Build it first:" -ForegroundColor Yellow
    Write-Host "  cargo install --path cli --force" -ForegroundColor Yellow
    exit 1
}

# 3. Verify model paths from agents/models.yaml
$modelsYaml = Join-Path $root "agents\models.yaml"
if (-not (Test-Path $modelsYaml)) {
    Write-Host "Missing agents/models.yaml" -ForegroundColor Red
    exit 1
}

$content = Get-Content $modelsYaml -Raw
$executable = ([regex]::Match($content, 'executable:\s*"([^"]+)"')).Groups[1].Value -replace '\\\\', '\'
$modelPath  = ([regex]::Match($content, 'path:\s*"([^"]+)"')).Groups[1].Value     -replace '\\\\', '\'

if (-not (Test-Path $executable)) {
    Write-Host "llama-server not found at: $executable" -ForegroundColor Red
    Write-Host "Edit agents/models.yaml to set the correct executable path." -ForegroundColor Yellow
    exit 1
}
if (-not (Test-Path $modelPath)) {
    Write-Host "GGUF model not found at: $modelPath" -ForegroundColor Red
    Write-Host "Edit agents/models.yaml to set the correct model path." -ForegroundColor Yellow
    exit 1
}

# 4. Dispatch mode
if ($Ask) {
    & $scos ask $Ask
}
elseif ($Mcp) {
    if ($Sse) {
        Write-Host "SkyCodeOS MCP (SSE) on http://0.0.0.0:11435/mcp" -ForegroundColor Green
        & $scos mcp --sse --port 11435
    } else {
        & $scos mcp
    }
}
else {
    $activeProfile = (& $scos profile show 2>&1) -join " "
    Write-Host ""
    Write-Host "SkyCodeOS API - starting on http://127.0.0.1:$Port" -ForegroundColor Green
    Write-Host "  Chat completions:  POST /v1/chat/completions" -ForegroundColor Cyan
    Write-Host "  Models list:       GET  /v1/models"            -ForegroundColor Cyan
    Write-Host "  Event stream:      GET  /v1/events  (SSE)"     -ForegroundColor Cyan
    Write-Host "  Active profile:    $activeProfile"             -ForegroundColor Cyan
    Write-Host "  Model executable:  $executable"                -ForegroundColor DarkGray
    Write-Host "  Model file:        $modelPath"                 -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "Ctrl+C to stop." -ForegroundColor Yellow
    Write-Host ""
    & $scos serve --host 0.0.0.0 --port $Port
}
