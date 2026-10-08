# datum tab completion for bash. Every answer comes from `datum __complete`,
# so this file never goes stale. The bash-completion package loads it from
# ~/.local/share/bash-completion/completions/datum (`datum completions
# install`), or: source <(datum completions bash)
_datum() {
  local cur words cword
  if declare -F _get_comp_words_by_ref >/dev/null; then
    _get_comp_words_by_ref -n =: cur words cword
  else
    words=("${COMP_WORDS[@]}") cword=$COMP_CWORD cur=${COMP_WORDS[COMP_CWORD]}
  fi
  local IFS=$'\n'
  # Ask the datum that installed this file, not whatever `datum` names now
  # (da-ejoes). With none, or that one gone: `datum`, or its alias.
  local exe=@DATUM_EXE@
  if [[ -x $exe ]]; then
    COMPREPLY=($("$exe" __complete "$cword" -- "${words[@]}" 2>/dev/null))
  else
    COMPREPLY=($(datum __complete "$cword" -- "${words[@]}" 2>/dev/null))
  fi
  [[ ${#COMPREPLY[@]} -eq 1 && ${COMPREPLY[0]} == *= ]] && compopt -o nospace 2>/dev/null
  declare -F __ltrim_colon_completions >/dev/null && __ltrim_colon_completions "$cur"
  return 0
}
complete -F _datum datum
