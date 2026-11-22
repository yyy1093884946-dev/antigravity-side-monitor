$ErrorActionPreference = 'Stop'

$rustc = Get-Command rustc -ErrorAction SilentlyContinue
if (-not $rustc) {
  throw "rustc not found in PATH"
}

& rustc -C opt-level=s -C lto=thin -C panic=abort -C strip=symbols -C debuginfo=0 -o AntigravityMonitor.exe main.rs
if ($LASTEXITCODE -ne 0) {
  throw "Rust compilation failed with exit code $LASTEXITCODE"
}

$fi = Get-Item "AntigravityMonitor.exe"
Write-Host ("Build succeeded: {0} ({1} bytes)" -f $fi.FullName, $fi.Length)
