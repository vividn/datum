# chorerescue N [chore…] — spread the chores that are due over the coming
# days, at most N a day, most overdue first. A day that already holds chores
# (future ones, or due ones left where they are) takes that many fewer.
#
# Each move is a shirk, `occur <chore> -T next_date=+D`, so the date moves
# and the chore does not count as done. Name chores to rescue only those.
# It prints the plan and asks before writing anything.
#
# Days are dates: a chore due at a time (`next_time`) counts on its date
# (UTC, as the kit stores it), and a rescue moves it to a date, so it is a
# date chore until it is next done.
#
# Needs the chores kit and jq. `source kits/chores-rescue.zsh` from zsh;
# CHORE_DATUM overrides the command, e.g. CHORE_DATUM='datum --db home'.
chorerescue() {
	local n=$1
	[[ $n == <1-> ]] || { print -u2 "usage: chorerescue N [chore…]   (N ≥ 1 a day)"; return 2 }
	shift
	local -a dtm=(${=CHORE_DATUM:-datum})
	local json plan
	json=$($dtm chorelist --all --show json) || return
	plan=$(jq -r --argjson n $n --arg today "$(date +%F)" '
		def day($d): $today + "T00:00:00Z" | fromdateiso8601 + $d * 86400 | strftime("%Y-%m-%d");
		($ARGS.positional) as $pick
		| [.[] | select(.due != null) | .day = .due[:10]] as $all
		| def picked: ($pick | length) == 0 or (.field | IN($pick[]));
		[$all[] | select(.day <= $today and picked)] | sort_by(.due) as $due
		# A chore left where it is holds its day: today if it is due, else its date.
		| (reduce ($all[] | select(picked and .day <= $today | not)
			| [.day, $today] | max) as $d ({}; .[$d] += 1)) as $held
		| reduce $due[] as $c ({d: 0, used: 0, out: []};
			until($n - ($held[day(.d)] // 0) - .used > 0; .d += 1 | .used = 0)
			| .out += [[$c.field, .d, $c.due]] | .used += 1)
		| .out[] | @tsv' --args "$@" <<< $json) || return
	[[ -n $plan ]] || { print "Nothing due."; return 0 }
	local field d due
	while IFS=$'\t' read -r field d due; do
		print -r -- "$field  (due $due) → +$d"
	done <<< $plan
	local ok
	read -r "ok?Shirk these? [y/N] "
	[[ $ok == [yY]* ]] || return 1
	while IFS=$'\t' read -r field d due; do
		$dtm occur $field -T next_date=+$d || return
	done <<< $plan
}
