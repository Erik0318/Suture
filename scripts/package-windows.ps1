$ErrorActionPreference = "Stop"

$Version = if ($args.Count -gt 0) { $args[0] } else { "1.0.0" }
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Dist = Join-Path $Root "dist-windows"
$App = Join-Path $Dist "app"
$UcrtBin = "C:\msys64\ucrt64\bin"

Remove-Item $Dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item $App -ItemType Directory -Force | Out-Null

Copy-Item (Join-Path $Root "target\release\suture.exe") (Join-Path $App "Suture.exe")
foreach ($Tool in @("ffmpeg.exe", "ffprobe.exe", "curl.exe", "cd-paranoia.exe")) {
    $Source = Join-Path $UcrtBin $Tool
    if (-not (Test-Path $Source)) { throw "Required sidecar is missing: $Source" }
    Copy-Item $Source $App
}
Copy-Item (Join-Path $Root "LICENSE") $App
Copy-Item (Join-Path $Root "THIRD_PARTY_NOTICES.md") $App

$Objdump = Join-Path $UcrtBin "objdump.exe"
$Queue = [System.Collections.Generic.Queue[string]]::new()
Get-ChildItem $App -Filter "*.exe" | ForEach-Object { $Queue.Enqueue($_.FullName) }
$Seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
while ($Queue.Count -gt 0) {
    $Binary = $Queue.Dequeue()
    foreach ($Line in (& $Objdump -p $Binary)) {
        if ($Line -match "DLL Name:\s*(.+)$") {
            $Name = $Matches[1].Trim()
            $Source = Join-Path $UcrtBin $Name
            if ((Test-Path $Source) -and $Seen.Add($Name)) {
                $Target = Join-Path $App $Name
                Copy-Item $Source $Target
                $Queue.Enqueue($Target)
            }
        }
    }
}

$Cert = Join-Path $Root "ca-certificates.crt"
if (-not (Test-Path $Cert)) { throw "ca-certificates.crt was not downloaded" }
Copy-Item $Cert $App

$Iscc = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $Iscc)) { throw "Inno Setup 6 is unavailable" }
& $Iscc (Join-Path $Root "packaging\windows\Suture.iss")

$Installer = Join-Path $Dist "Suture${Version}-Windows-x86_64-Setup.exe"
if (-not (Test-Path $Installer)) { throw "Windows installer was not created" }
$Hash = (Get-FileHash $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$Hash  $([IO.Path]::GetFileName($Installer))" | Set-Content "$Installer.sha256" -Encoding ascii
