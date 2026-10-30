#!/usr/bin/env bash
# Resize a running test window through many shapes and save the app's own
# render of every page at each size. Repo-only: the resize goes through the
# linux-computer-use skill's KWin helper (`lcu window geometry`), which acts
# only on the window you name. The app never resizes or captures other apps.
#
#   target/debug/warehouse-cosmic-cli ui start --config-dir PRIVATE_DIR
#   verification/layout-sweep.sh INSTANCE 'id:{KWIN-WINDOW-ID}' OUT_DIR [WxH ...]
#
# The window goes to the top-left of MONITOR (default DP-1). Sizes larger than
# the monitor minus RESERVE px at the bottom (the panel) are shrunk to fit, so
# no capture is of a window running off the screen.
#
# Output per scene and requested size:
#   OUT_DIR/<scene>-<W>x<H>.png         the app's own render (`ui capture`)
#   OUT_DIR/screen/<scene>-<W>x<H>.png  what the screen shows (Spectacle via
#                                       `lcu capture shot --window`)
# The two should match. The own render is taken before the screenshot raises
# the window, so it can show the unfocused (grey) header. A difference beyond
# that is a rendering bug: it once dropped the header title (see header_start).
# OUT_DIR/sizes.txt records the size asked for, fitted, and applied by KWin.
set -euo pipefail
instance=$1 window=$2 out=$3
shift 3
sizes=("$@")
[ ${#sizes[@]} -gt 0 ] || sizes=(1600x850 1240x800 940x560 768x820 600x720 480x800 1500x400 380x640)
cli=${CLI:-$(dirname "$0")/../target/debug/warehouse-cosmic-cli}
lcu=${LCU:-$HOME/.claude/skills/linux-computer-use/lcu}
monitor=${MONITOR:-DP-1} reserve=${RESERVE:-50}
read -r x y mw mh < <("$lcu" capture screens |
    jq -r --arg m "$monitor" '.[] | select(.name == $m) | "\(.x) \(.y) \(.width) \(.height)"')
[ -n "${x:-}" ] || { echo "no monitor named $monitor" >&2; exit 1; }
mh=$((mh - reserve))
mkdir -p "$out/screen"
: >"$out/sizes.txt"
ui() { "$cli" ui "$@" --instance "$instance" >/dev/null; }
shoot() {
    ui capture "$out/$1.png" --overwrite
    "$lcu" capture shot "$out/screen/$1.png" --window "$window" >/dev/null
}
first_package() {
    "$cli" ui controls --instance "$instance" |
        grep -o '"package:[^"]*"' | head -1 | tr -d '"'
}
for size in "${sizes[@]}"; do
    w=$((${size%x*} < mw ? ${size%x*} : mw)) h=$((${size#*x} < mh ? ${size#*x} : mh))
    "$lcu" window geometry "$window" "$x" "$y" "$w" "$h" >/dev/null
    "$lcu" window wait-stable "$window" >/dev/null
    got=$("$lcu" window info "$window" | grep -Eo '"(x|y|width|height)": [0-9.]+' | head -4 | grep -Eo '[0-9.]+' | paste -sd ' ')
    echo "$size -> fit ${w}x$h at $x,$y -> applied x y w h: $got" | tee -a "$out/sizes.txt"
    for page in packages remotes data snapshots install; do
        ui navigate "$page"
        [ "$page" = packages ] && ui activate packages.back 2>/dev/null || true
        # Page switches load data; capture only after it settles.
        ui wait || true
        sleep 0.8
        shoot "$page-$size"
    done
    ui navigate packages
    ui activate "$(first_package)"
    ui wait || true
    sleep 0.4
    shoot "package-detail-$size"
done
