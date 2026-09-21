#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

logo="$script_dir/logo.png"
font="$script_dir/MysticCards.ttf"
output="$script_dir/icon.png"

size=512
logo_size=280

[[ -f "$logo" ]] || {
  echo "Missing logo: $logo" >&2
  exit 1
}

[[ -f "$font" ]] || {
  echo "Missing font: $font" >&2
  exit 1
}

magick -size "${size}x${size}" xc:"#140a18" \
  \( -size "${size}x${size}" radial-gradient:"#3a2148-#09060c" \
     -rotate 90 \
     -evaluate Multiply 0.75 \) \
  -compose screen -composite \
  -fill "#2a1633" \
  -draw "rectangle 0,0 175,512" \
  \( "$logo" \
     -resize "${logo_size}x${logo_size}" \
     -channel A -blur 0x20 +channel \
     -fill "#a56fd0" -colorize 100 \) \
  -gravity north \
  -geometry +0+42 \
  -compose screen -composite \
  \( "$logo" -resize "${logo_size}x${logo_size}" \) \
  -gravity north \
  -geometry +0+48 \
  -compose over -composite \
  -font "$font" \
  -fill "#e4c8ff" \
  -pointsize 54 \
  -gravity north \
  -annotate +0+360 "Morrobroom" \
  "$output"

echo "Wrote $output"
