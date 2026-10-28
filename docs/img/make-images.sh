#!/usr/bin/env bash
# make-images.sh: draw the README's pictures from invented days on kits/sample.toml, with where you are behind each
# day and sleep across its middle (docs/img/readme-kit.toml). Nothing here is anyone's real record.
# Run from the repo root, in the daytime (at night, set TZ to a zone where it is day), with `datum` on PATH:
#   docs/img/make-images.sh
# It writes docs/img/dayview.svg (Wednesday to Friday, 14–16 October 2026) and docs/img/nowview.svg
# (the last three hours), from a throwaway home that it removes. Today's hours are relative to now,
# because a nowview window ends at the real clock, so only the nowview's clock labels change from one
# run to the next.
set -euo pipefail
OUT=${OUT:-docs/img}
T=$(mktemp -d)
trap 'rm -rf "${T:?}"' EXIT
export HOME=$T DATUM_HOME=$T/data DATUM_CONFIG_HOME=$T/config DATUM_SYNC_NO_DAEMON=1 NO_COLOR=1
d() { datum "$@" > /dev/null; }
d kit install docs/img/readme-kit.toml
d kit install kits/sample.toml
# The drinks' dots, half again the standard size (0.08 of the day), so they read at README width.
d setup coffee --glyph-scale 1.5
d setup water --glyph-scale 1.5
on() { local day=$1 time=$2; shift 2; d "$@" -d "$day" -t "$time"; }   # on DAY TIME COMMAND...

# Wednesday: at the office, walking there and back.
on 2026-10-13 23:35 switch environment home; on 2026-10-13 23:35 start sleep
on 2026-10-14 06:50 stop sleep
on 2026-10-14 07:10 occur coffee
on 2026-10-14 08:05 switch environment outside; on 2026-10-14 08:05 start walk;  on 2026-10-14 08:35 stop walk
on 2026-10-14 08:35 switch environment office
on 2026-10-14 08:45 start work;               on 2026-10-14 08:45 switch project datum
on 2026-10-14 10:00 occur water amount=250
on 2026-10-14 11:30 switch project email
on 2026-10-14 12:15 stop work;                on 2026-10-14 12:15 end project
on 2026-10-14 12:40 occur coffee
on 2026-10-14 13:00 start work;               on 2026-10-14 13:00 switch project admin
on 2026-10-14 14:30 switch project datum
on 2026-10-14 15:10 occur water amount=250
on 2026-10-14 17:20 stop work;                on 2026-10-14 17:20 end project
on 2026-10-14 17:25 switch environment outside; on 2026-10-14 17:25 start walk;  on 2026-10-14 17:55 stop walk
on 2026-10-14 17:55 switch environment home
on 2026-10-14 19:30 occur water amount=250
on 2026-10-14 21:15 start read;               on 2026-10-14 22:20 stop read
on 2026-10-14 23:05 start sleep

# Thursday: at home, a late start, a long walk at midday, work late into the night.
on 2026-10-15 08:40 stop sleep
on 2026-10-15 09:05 occur coffee
on 2026-10-15 09:30 start work;               on 2026-10-15 09:30 switch project datum
on 2026-10-15 11:10 occur coffee
on 2026-10-15 11:45 stop work;                on 2026-10-15 11:45 end project
on 2026-10-15 12:30 switch environment outside; on 2026-10-15 12:30 start walk;  on 2026-10-15 14:10 stop walk
on 2026-10-15 14:10 switch environment home
on 2026-10-15 14:15 occur water amount=250
on 2026-10-15 15:00 start work;               on 2026-10-15 15:00 switch project email
on 2026-10-15 16:00 stop work;                on 2026-10-15 16:00 end project
on 2026-10-15 16:30 start read;               on 2026-10-15 18:30 stop read
on 2026-10-15 19:00 switch environment outside; on 2026-10-15 19:00 start walk;  on 2026-10-15 19:40 stop walk
on 2026-10-15 19:40 switch environment home
on 2026-10-15 21:00 start work;               on 2026-10-15 21:00 switch project datum
on 2026-10-15 22:00 occur water amount=250
on 2026-10-15 23:30 stop work;                on 2026-10-15 23:30 end project
on 2026-10-16 00:20 start sleep

# Friday: a day out, no work.
on 2026-10-16 09:15 stop sleep
on 2026-10-16 09:40 occur coffee
on 2026-10-16 10:30 switch environment outside; on 2026-10-16 10:30 start walk;  on 2026-10-16 12:00 stop walk
on 2026-10-16 11:15 occur water amount=250
on 2026-10-16 12:30 occur water amount=250
on 2026-10-16 13:00 occur coffee
on 2026-10-16 14:00 start walk;               on 2026-10-16 15:30 stop walk
on 2026-10-16 16:40 switch environment home
on 2026-10-16 17:00 start read;               on 2026-10-16 18:00 stop read
on 2026-10-16 18:30 occur water amount=250
on 2026-10-16 21:30 start read;               on 2026-10-16 23:10 stop read
on 2026-10-16 23:50 start sleep
on 2026-10-17 08:30 stop sleep

# Today, in minutes before now: the office, with a short walk outside.
d start sleep -y -t 23:40
d stop sleep -t -400
d occur coffee -t -380
d switch environment outside -t -235; d start walk -t -235; d stop walk -t -210
d switch environment office -t -210
d start work -t -200;   d switch project datum -t -200
d occur coffee -t -150
d occur water amount=250 -t -110
d switch project email -t -70
d stop work -t -45;     d end project -t -45
d switch environment outside -t -45; d start walk -t -45; d stop walk -t -25
d switch environment office -t -25
d start work -t -20;    d switch project admin -t -20
d occur water amount=250 -t -10

datum dayview --start-date 2026-10-14 --end-date 2026-10-16 --width 1200 -o "$OUT/dayview.svg"
datum nowview --context 3h -o "$OUT/nowview.svg"
echo "wrote $OUT/dayview.svg and $OUT/nowview.svg"
