# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$VeilidRepository,
    [Parameter(Mandatory = $true)]
    [string]$WasmBindgen,
    [string]$OutputDirectory = "site/veilid-spike"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../../..")
$veilidRoot = Resolve-Path $VeilidRepository
$output = Join-Path $repoRoot $OutputDirectory
$rawWasm = Join-Path $veilidRoot "target/wasm32-unknown-unknown/release/veilid_wasm.wasm"

$bindgenVersion = & $WasmBindgen --version
if ($LASTEXITCODE -ne 0 -or $bindgenVersion -ne "wasm-bindgen 0.2.121") {
    throw "expected wasm-bindgen 0.2.121, got: $bindgenVersion"
}

Push-Location $veilidRoot
try {
    cargo +1.96.0 build --locked --release --package veilid-wasm `
        --target wasm32-unknown-unknown --features enable-protocol-wss
    if ($LASTEXITCODE -ne 0) {
        throw "Veilid WASM build failed"
    }
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path (Join-Path $output "pkg") | Out-Null
$staleTypeScript = @(
    (Join-Path $output "pkg/veilid_wasm.d.ts"),
    (Join-Path $output "pkg/veilid_wasm_bg.wasm.d.ts")
)
Remove-Item -LiteralPath $staleTypeScript -Force -ErrorAction SilentlyContinue
& $WasmBindgen --target web --weak-refs --no-typescript `
    --out-dir (Join-Path $output "pkg") $rawWasm
if ($LASTEXITCODE -ne 0) {
    throw "Veilid wasm-bindgen packaging failed"
}

Copy-Item (Join-Path $PSScriptRoot "index.html") (Join-Path $output "index.html")
Copy-Item (Join-Path $PSScriptRoot "probe.js") (Join-Path $output "probe.js")
