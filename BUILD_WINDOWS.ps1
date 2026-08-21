$ErrorActionPreference = 'Stop'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust/Cargo was not found. Install the stable x86_64-pc-windows-msvc Rust toolchain first.'
}

Write-Host '[1/2] Running protocol regression tests...'
cargo test

Write-Host '[2/2] Building release executable...'
cargo build --release

$exe = Join-Path $PSScriptRoot 'target\release\prospect-headless.exe'
if (-not (Test-Path $exe)) {
    throw "Build completed without expected executable: $exe"
}

Write-Host "Built: $exe"
Write-Host 'Proxy example:'
Write-Host '  .\target\release\prospect-headless.exe --mode proxy --bind 0.0.0.0:7788 --upstream 127.0.0.1:7777 --capture known-good-session.log'
