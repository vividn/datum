#compdef datum
# datum tab completion for zsh. Every answer comes from `datum __complete`,
# so this file never goes stale. Load it from a directory on $fpath
# (`datum completions install`) or with `source <(datum completions zsh)`.
_datum() {
  local -a vals keys cmd
  local line v d out rc
  # Completion runs with aliases off (compinit's NO_aliases), so a `datum`
  # that is only an alias is expanded here, as the prompt would.
  eval "cmd=(${aliases[datum]:-datum})" 2>/dev/null || cmd=(datum)
  out=$($cmd __complete --raw $((CURRENT - 1)) -- "${words[@]}" 2>/dev/null)
  rc=$?
  if (( rc == 127 )); then
    _message -r "datum completion: \`$cmd\` not found${${aliases[datum]:+ (alias datum)}:- on PATH}"
    return 1
  elif (( rc )); then
    _message -r "datum completion: \`$cmd __complete\` failed (exit $rc)"
    return 1
  fi
  for line in "${(@f)out}"; do
    [[ -n $line ]] || continue
    v=${line%%$'\t'*} d=
    [[ $line == *$'\t'* ]] && d=${line#*$'\t'}
    if [[ $v == *= ]]; then keys+=("${v//:/\\:}:$d"); else vals+=("${v//:/\\:}:$d"); fi
  done
  (( $#vals )) && _describe -t values datum vals
  (( $#keys )) && _describe -t keys key keys -S ''
  return 0
}
if [[ $zsh_eval_context[-1] == loadautofunc ]]; then
  _datum "$@"
else
  compdef _datum datum
fi
