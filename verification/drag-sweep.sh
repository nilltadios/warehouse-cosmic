#!/usr/bin/env bash
# Resize a test window by dragging its corners with the pointer, as a person
# would, starting from wherever the window is now. Repo-only: input goes
# through the linux-computer-use skill (`lcu input`), aimed at the resize
# border of the window you name; nothing else is clicked.
#
#   verification/drag-sweep.sh 'id:{KWIN-WINDOW-ID}' OUT_DIR [WxH ...]
#
# For each size it drags the bottom-right corner, then the top-left corner for
# whatever the screen edge left over. Hand resizes respect the app's minimum
# window size, so a smaller request is raised to it; OUT_DIR/sizes.txt records
# the size asked for and the size reached. OUT_DIR/screen-<W>x<H>.png is a screen
# capture of the window at each step.
set -euo pipefail
window=$1 out=$2
shift 2
sizes=("$@")
[ ${#sizes[@]} -gt 0 ] || sizes=(1600x850 1240x800 940x560 768x820 600x720 480x800 1500x400 380x640)
lcu=${LCU:-$HOME/.claude/skills/linux-computer-use/lcu}
reserve=${RESERVE:-50} # panel height at the bottom of the monitor
# The app's minimum window size (src/main.rs size_limits). Hand resizes stop
# there, so targets are raised to it and every drag is expected to move.
min_w=${MIN_W:-380} min_h=${MIN_H:-400}
# Grab points inside the resize border, checked by hovering for the resize
# cursor (KWin 6.7, libcosmic at scale 1.6). The border is 2-5 px deep (at
# 6 px the cursor is the normal arrow). At the bottom-right corner 2-3 px
# falls outside the rounded window and the press lands in the window behind;
# 4 px resizes both ways. The top-left corner takes 3 px.
inset_br=4 inset_tl=3
mkdir -p "$out"
: >"$out/sizes.txt"

geometry() { # -> x y w h output (integers)
    "$lcu" window info "$window" |
        jq -r '.[0] | "\(.x|round) \(.y|round) \(.width|round) \(.height|round) \(.output)"'
}
monitor() { # output -> x y w h
    "$lcu" capture screens |
        jq -r --arg m "$1" '.[] | select(.name == $m) | "\(.x) \(.y) \(.width) \(.height)"'
}
# Press at (x, y), move by (dx, dy) in small steps, release. A drag that
# leaves the window unchanged stops the sweep: the press may have missed the
# resize border, and further presses would land in other windows.
# (ax, ay) is a point just inside the window: the pointer moves there first,
# because the app picks the resize edge from pointer motion inside its window
# and a single jump onto the border from outside does not register.
drag() {
    local x=$1 y=$2 ax=$3 ay=$4 dx=$5 dy=$6 steps i sx sy args=()
    [ "$dx" -eq 0 ] && [ "$dy" -eq 0 ] && return
    # Steps of at most 8 px: a large first move arrives before KWin has
    # started the resize, and the drag does nothing.
    steps=$(((${dx#-} > ${dy#-} ? ${dx#-} : ${dy#-}) / 8 + 1))
    args=(move "$ax" "$ay" sleep 100 move "$x" "$y" sleep 200 down left sleep 300)
    for ((i = 1; i <= steps; i++)); do
        sx=$((dx * i / steps - dx * (i - 1) / steps))
        sy=$((dy * i / steps - dy * (i - 1) / steps))
        args+=(rmove "$sx" "$sy" sleep 12)
    done
    args+=(sleep 400 up left)
    local before after
    before=$(geometry)
    "$lcu" window activate "$window" >/dev/null
    sleep 0.3
    "$lcu" input "${args[@]}"
    sleep 0.4
    after=$(geometry)
    if [ "$before" = "$after" ]; then
        echo "drag from $x,$y by $dx,$dy left the window at $after; stopping" | tee -a "$out/sizes.txt" >&2
        exit 1
    fi
}
# Grab point for a resize by (dx, dy) from the bottom-right (side=br) or
# top-left (side=tl): the corner when both change, otherwise the middle of
# the edge that moves. A corner next to the screen edge reads as the side
# edge alone, so a height-only drag from there does nothing.
grab() {
    local side=$1 dx=$2 dy=$3 x y w h px py
    read -r x y w h _ < <(geometry)
    if [ "$side" = br ]; then
        px=$((x + w - inset_br)) py=$((y + h - inset_br))
    else
        px=$((x + inset_tl)) py=$((y + inset_tl))
    fi
    [ "$dx" -eq 0 ] && px=$((x + w / 2))
    [ "$dy" -eq 0 ] && py=$((y + h / 2))
    # Approach from 12 px further inside, towards the window's middle.
    echo "$px $py $((px + (px < x + w / 2 ? 12 : -12))) $((py + (py < y + h / 2 ? 12 : -12)))"
}
# Clamp to [lo, hi]; moves under 4 px become 0 (too small to start a resize).
clamp() {
    local v=$1 lo=$2 hi=$3
    ((v < lo)) && v=$lo
    ((v > hi)) && v=$hi
    ((v > -4 && v < 4)) && v=0
    echo "$v"
}

for size in "${sizes[@]}"; do
    tw=${size%x*} th=${size#*x}
    ((tw < min_w)) && tw=$min_w
    ((th < min_h)) && th=$min_h
    read -r x y w h output < <(geometry)
    read -r mx my mw mh < <(monitor "$output")
    mh=$((mh - reserve))
    # Bottom-right corner: grow up to the screen edge, or shrink.
    dx=$(clamp $((tw - w)) -$((w - 1)) $((mx + mw - x - w)))
    dy=$(clamp $((th - h)) -$((h - 1)) $((my + mh - y - h)))
    drag $(grab br "$dx" "$dy") "$dx" "$dy"
    # Top-left corner: whatever is still missing, up to the screen edge.
    read -r x y w h output < <(geometry)
    dx=$(clamp $((w - tw)) $((mx - x)) 0)
    dy=$(clamp $((h - th)) $((my - y)) 0)
    drag $(grab tl "$dx" "$dy") "$dx" "$dy"
    read -r x y w h output < <(geometry)
    echo "$size -> reached ${w}x$h at $x,$y" | tee -a "$out/sizes.txt"
    "$lcu" capture shot "$out/screen-$size.png" --window "$window" >/dev/null
done
