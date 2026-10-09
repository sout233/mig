param([switch]$Online, [switch]$Playlist)
$ErrorActionPreference = 'Stop'

$projectDirectory = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$outputDirectory = Join-Path $projectDirectory ('target\ui-verification\run-' + [guid]::NewGuid().ToString('N'))
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoExecutable = if ($cargoCommand) {
    $cargoCommand.Source
} else {
    Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
}

Push-Location $projectDirectory
$previousVerifyDirectory = $env:MIG_VERIFY_UI
$previousDataDirectory = $env:MIG_MUSIC_DATA_DIR
$previousOnline = $env:MIG_VERIFY_ONLINE
$previousPlaylist = $env:MIG_VERIFY_PLAYLIST

try {
    & $cargoExecutable build --features ui-verify
    if ($LASTEXITCODE -ne 0) {
        throw '验证程序构建失败。'
    }
    [System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
    $env:MIG_VERIFY_UI = $outputDirectory
    $env:MIG_MUSIC_DATA_DIR = Join-Path $outputDirectory 'data'
    if ($Online) {
        $env:MIG_VERIFY_ONLINE = '1'
    }
    if ($Playlist) {
        $env:MIG_VERIFY_PLAYLIST = '1'
    }
    $verificationProcess = Start-Process -FilePath (Join-Path $projectDirectory 'target\debug\mig.exe') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $outputDirectory 'stderr.txt')
    if (-not $verificationProcess.WaitForExit(180000)) {
        Stop-Process -Id $verificationProcess.Id
        throw '界面验证超时，请检查音频设备和 stderr.txt。'
    }
    $reportPath = Join-Path $outputDirectory 'result.txt'
    if (-not (Test-Path -LiteralPath $reportPath)) {
        throw "验证未完成，日志位于 $outputDirectory"
    }
    $report = [System.IO.File]::ReadAllText($reportPath, [System.Text.Encoding]::UTF8)
    if (-not $report.StartsWith('PASS:')) {
        throw $report
    }
    Write-Output $report
    Write-Output "截图与验证结果：$outputDirectory"
} finally {
    $env:MIG_VERIFY_UI = $previousVerifyDirectory
    $env:MIG_MUSIC_DATA_DIR = $previousDataDirectory
    $env:MIG_VERIFY_ONLINE = $previousOnline
    $env:MIG_VERIFY_PLAYLIST = $previousPlaylist
    Pop-Location
}
