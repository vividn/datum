#compdef datum
# datum tab completion for zsh. Every answer comes from `datum __complete`,
# so this file never goes stale. Load it from a directory on $fpath
# (`datum completions install`) or with `source <(datum completions zsh)`.
_datum() {
  local -a vals keys
  local line v d
  for line in "${(@f)$(datum __complete --raw $((CURRENT - 1)) -- "${words[@]}" 2>/dev/null)}"; do
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
