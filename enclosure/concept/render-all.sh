#!/bin/sh
# Renders both shell colours: a 10 s video and a still each. Run from the
# repository root. Needs Blender 5.x and ffmpeg.
set -e
BLENDER="${BLENDER:-/c/Program Files/Blender Foundation/Blender 5.2/blender.exe}"
OUT="${OUT:-$(pwd -W)/enclosure/concept/renders}"
for v in white black; do
    "$BLENDER" -b -P enclosure/concept/concept.py -- --variant $v --mode still --frame 300 --samples 64 --out "$OUT"
    "$BLENDER" -b -P enclosure/concept/concept.py -- --variant $v --mode anim --samples 32 --out "$OUT"
    ffmpeg -y -loglevel error -framerate 30 -i "$OUT/$v/frame_%04d.png" -c:v libx264 -pix_fmt yuv420p -crf 18 "$OUT/concept-$v.mp4"
done
