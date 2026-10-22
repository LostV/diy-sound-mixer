# Regenerates the LVGL fonts in this folder from Inter (SIL OFL 1.1, see OFL.txt).
# LVGL's built-in fonts are ASCII only; these include Latin-1, so æ ø å work.
#
#   ./gen-fonts.ps1 -Tools <folder>
#
# <folder> gets a throwaway npm install of @fontsource/inter and lv_font_conv.

param([Parameter(Mandatory)] [string] $Tools)

$ErrorActionPreference = "Stop"
$out = $PSScriptRoot
New-Item -ItemType Directory -Force $Tools | Out-Null
Push-Location $Tools
try {
    if (-not (Test-Path package.json)) { npm init -y | Out-Null }
    npm install --no-audit --no-fund @fontsource/inter@5 lv_font_conv@1.5 | Out-Null
    $files = Join-Path $Tools "node_modules/@fontsource/inter/files"
    $conv = Join-Path $Tools "node_modules/.bin/lv_font_conv.cmd"

    # Text: printable ASCII, Latin-1 (æøå and friends), en dash, ellipsis, minus.
    $text = "0x20-0x7E,0xA0-0xFF,0x2013,0x2026,0x2212"
    # Big numbers only need digits and the percent sign.
    $digits = "0x20,0x25,0x2D,0x30-0x39,0x2013"

    $fonts = @(
        @{ name = "inter_14";        weight = 400; size = 14; range = $text },
        @{ name = "inter_16";        weight = 400; size = 16; range = $text },
        @{ name = "inter_semi_20";   weight = 600; size = 20; range = $text },
        @{ name = "inter_light_44";  weight = 300; size = 44; range = $digits }
    )
    foreach ($f in $fonts) {
        $src = Join-Path $files "inter-latin-$($f.weight)-normal.woff"
        $dst = Join-Path $out "$($f.name).c"
        & $conv --font $src -r $f.range --size $f.size --bpp 4 --format lvgl --no-compress `
            --lv-include "lvgl.h" --lv-font-name $f.name -o $dst
        if ($LASTEXITCODE -ne 0) { throw "lv_font_conv failed for $($f.name)" }
        Write-Host "wrote $dst"
    }
    Copy-Item (Join-Path $Tools "node_modules/@fontsource/inter/LICENSE") (Join-Path $out "OFL.txt") -Force
}
finally {
    Pop-Location
}
