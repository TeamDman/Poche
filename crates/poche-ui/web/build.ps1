# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

[CmdletBinding()]
param(
    [string]$OutputDirectory = "site/replay"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../../..")
$output = Join-Path $repoRoot $OutputDirectory
$wasm = Join-Path $repoRoot "target/wasm32-unknown-unknown/release/poche_ui.wasm"

Push-Location $repoRoot
try {
    $wasmBindgenVersion = wasm-bindgen --version
    if ($LASTEXITCODE -ne 0 -or $wasmBindgenVersion -ne "wasm-bindgen 0.2.126") {
        throw "expected wasm-bindgen 0.2.126, got: $wasmBindgenVersion"
    }

    cargo build --locked --release --package poche-ui --lib --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) {
        throw "cargo WASM build failed"
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $output "pkg") | Out-Null
    wasm-bindgen --target web --no-typescript --out-dir (Join-Path $output "pkg") $wasm
    if ($LASTEXITCODE -ne 0) {
        throw "wasm-bindgen packaging failed"
    }

    Copy-Item (Join-Path $PSScriptRoot "index.html") (Join-Path $output "index.html")
} finally {
    Pop-Location
}
