# Builds the CrowPanel firmware, and uploads it when a port is given.
#
#   ./build.ps1             build into build/
#   ./build.ps1 -Port COM7  build and upload (quit the mixer app first)

param([string] $Port)

$ErrorActionPreference = "Stop"
$fqbn = "esp32:esp32:esp32s3:FlashSize=16M,PSRAM=opi,PartitionScheme=app3M_fat9M_16MB"
$sketch = $PSScriptRoot
$out = Join-Path $sketch "build"
# LVGL finds our lv_conf.h through this include path; it must be absolute.
$include = "-I" + $sketch.Replace('\', '/')

arduino-cli compile --fqbn $fqbn --output-dir $out `
    --build-property "compiler.cpp.extra_flags=$include" `
    --build-property "compiler.c.extra_flags=$include" $sketch
if ($LASTEXITCODE -ne 0) { throw "compile failed" }

if ($Port) {
    arduino-cli upload --fqbn $fqbn -p $Port --input-dir $out $sketch
    if ($LASTEXITCODE -ne 0) { throw "upload failed" }
}
