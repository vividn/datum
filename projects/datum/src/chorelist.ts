// function chorelist {
// #	{
// 	all_chores="$(couch task/_design/chores/_view/chores'?group=true' | jq '.rows |=sort_by(.value.next) | .rows[] | [.value.lastDone[0:10], .value.next[0:10], .key] | @tsv' --raw-output)"
// 	if [ $? -gt 0 ]; then
// 		return
// 	fi
// 	now="$(date +"%Y-%m-%d")"
// 	chores_due="$(awk -v FS='\t' -v OFS='\t' '$2 != "" && $2 <= "'"$now"'"' <<< "$all_chores")"
// 	future_chores="$(awk -v FS='\t' -v OFS='\t' '$2 != "" && $2 > "'"$now"'"' <<< "$all_chores")"
// 	ten_dash="----------"
// 	header="Last Done \tNext Date \tChore"
// 	echo "$header"
// 	if [ ${#chores_due} -eq 0 ]; then
// 		echo -n "$(tput setaf 2)"
// 		echo "COMPLETED!\tGOOD WORK!\t\\(◦'⌣'◦)/"
// 	else
// 		echo -n "$(tput setaf 1)"
// 		echo "$chores_due"
// 		echo -n "$(tput sgr0)"
// 		echo -e "$ten_dash\t$ten_dash\t$ten_dash"
// 	fi
// 	echo "$future_chores"
// 	echo -n "$(tput sgr0)"
// #	} | columnify
// }
