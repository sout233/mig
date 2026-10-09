param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'

$projectDirectory = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$outputDirectory = Join-Path $projectDirectory ('target\startup-verification\' + [guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$previousDataDirectory = $env:MIG_MUSIC_DATA_DIR
$process = $null

Push-Location $projectDirectory

try {
    if (-not $NoBuild) {
        & cargo build -j 2
        if ($LASTEXITCODE -ne 0) {
            throw '启动验证构建失败'
        }
    }
    $env:MIG_MUSIC_DATA_DIR = Join-Path $outputDirectory 'data'
    $process = Start-Process -FilePath (Join-Path $projectDirectory 'target\debug\mig.exe') -WindowStyle Normal -PassThru -RedirectStandardError (Join-Path $outputDirectory 'stderr.txt') -RedirectStandardOutput (Join-Path $outputDirectory 'stdout.txt')
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt 8 -and -not $process.HasExited) {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    if ($process.HasExited) {
        $log = [System.IO.File]::ReadAllText((Join-Path $outputDirectory 'stderr.txt'), [System.Text.Encoding]::UTF8)
        if ($log.Length -gt 2000) {
            $log = $log.Substring(0, 2000)
        }
        throw "启动失败，退出码 $($process.ExitCode)：$log"
    }
    if ($process.MainWindowHandle -eq 0 -or -not $process.Responding) {
        throw '未检测到可响应的音乐窗口'
    }
    Write-Output 'PASS: 音乐窗口成功创建，持续运行 8 秒且可响应'
    Write-Output "日志：$outputDirectory"
} finally {
    if ($process -and -not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(2000)) {
            Stop-Process -Id $process.Id
        }
    }
    $env:MIG_MUSIC_DATA_DIR = $previousDataDirectory
    Pop-Location
}
