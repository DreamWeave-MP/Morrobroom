#!/usr/bin/env bash
set -euo pipefail

logo="logo.png"
output="morrobroom_header.png"

width=1300
height=372

magick -size "${width}x${height}" xc:"#120914" \
  \( -size "${width}x${height}" radial-gradient:"#2a1633-#09060b" \
     -rotate 90 \
     -evaluate Multiply 0.8 \) \
  -compose screen -composite \
  \( -size "${width}x${height}" xc:none \
     -fill "rgba(180,120,220,0.10)" \
     -draw "rectangle 0,0 ${width},${height}" \) \
  -compose over -composite \
  \( "$logo" -resize 260x260 \) \
  -geometry +70+56 -compose over -composite \
  -font "/var/home/s3kshun8/GitHub/rust-reimplementations/morrobroom/MysticCards.ttf" \
  -fill "#e4c8ff" \
  -pointsize 74 \
  -gravity northwest \
  -annotate +395+120 "MORROBROOM" \
  -font "/var/home/s3kshun8/GitHub/rust-reimplementations/morrobroom/MysticCards.ttf" \
  -fill "#c7afd9" \
  -pointsize 26 \
  -annotate +400+205 "OpenMW level compiler, lightmapper, and NIF-to-TrenchBroom importer" \
  "$output"
