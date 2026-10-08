# datum tab completion for fish. Every answer comes from `datum __complete`,
# so this file never goes stale. fish loads it from
# ~/.config/fish/completions/datum.fish (`datum completions install`).
function __datum_complete
    set -l words (commandline -opc) (commandline -ct)
    # Ask the datum that installed this file, not whatever `datum` names
    # now (da-ejoes). With none, or that one gone: `datum`.
    set -l exe @DATUM_EXE@
    test -x "$exe"; or set exe datum
    $exe __complete --raw (math (count $words) - 1) -- $words 2>/dev/null
end
complete -c datum -f -a '(__datum_complete)'
