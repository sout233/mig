param([switch]$Release)
$ErrorActionPreference = 'Stop'

$projectDirectory = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoExecutable = if ($cargoCommand) {
    $cargoCommand.Source
} else {
    Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
}

Push-Location $projectDirectory

try {
    if ($Release) {
        & $cargoExecutable build --release -j 2
        $profile = 'release'
    }
    else {
        & $cargoExecutable build -j 2
        $profile = 'debug'
    }
    if ($LASTEXITCODE -ne 0) {
        throw '构建失败。'
    }

    Write-Output "已生成 target\$profile\mig.exe，在线功能使用内置 JavaScript 引擎，无需 Node.js。"
} finally {
    Pop-Location
}
