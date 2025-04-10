
#--------------------------------------------------------------------#
# Highlighting                                                       #
#--------------------------------------------------------------------#

# If there was a highlight, remove it
_q_autosuggest_highlight_reset() {
	typeset -g _Q_AUTOSUGGEST_LAST_HIGHLIGHT

	if [[ -n "$_Q_AUTOSUGGEST_LAST_HIGHLIGHT" ]]; then
		region_highlight=("${(@)region_highlight:#$_Q_AUTOSUGGEST_LAST_HIGHLIGHT}")
		unset _Q_AUTOSUGGEST_LAST_HIGHLIGHT
	fi
}

# If there's a suggestion, highlight it
_q_autosuggest_highlight_apply() {
	typeset -g _Q_AUTOSUGGEST_LAST_HIGHLIGHT

	if (( $#POSTDISPLAY )); then
		typeset -g _Q_AUTOSUGGEST_LAST_HIGHLIGHT="$#BUFFER $(($#BUFFER + $#POSTDISPLAY)) $Q_AUTOSUGGEST_HIGHLIGHT_STYLE"
		region_highlight+=("$_Q_AUTOSUGGEST_LAST_HIGHLIGHT")
	else
		unset _Q_AUTOSUGGEST_LAST_HIGHLIGHT
	fi
}
