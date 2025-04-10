
#--------------------------------------------------------------------#
# Widget Helpers                                                     #
#--------------------------------------------------------------------#

_q_autosuggest_incr_bind_count() {
	typeset -gi bind_count=$((_Q_AUTOSUGGEST_BIND_COUNTS[$1]+1))
	_Q_AUTOSUGGEST_BIND_COUNTS[$1]=$bind_count
}

# Bind a single widget to an autosuggest widget, saving a reference to the original widget
_q_autosuggest_bind_widget() {
	typeset -gA _Q_AUTOSUGGEST_BIND_COUNTS

	local widget=$1
	local autosuggest_action=$2
	local prefix=$Q_AUTOSUGGEST_ORIGINAL_WIDGET_PREFIX

	local -i bind_count

	# Save a reference to the original widget
	case $widgets[$widget] in
		# Already bound
		user:_q_autosuggest_(bound|orig)_*)
			bind_count=$((_Q_AUTOSUGGEST_BIND_COUNTS[$widget]))
			;;

		# User-defined widget
		user:*)
			_q_autosuggest_incr_bind_count $widget
			zle -N $prefix$bind_count-$widget ${widgets[$widget]#*:}
			;;

		# Built-in widget
		builtin)
			_q_autosuggest_incr_bind_count $widget
			eval "_q_autosuggest_orig_${(q)widget}() { zle .${(q)widget} }"
			zle -N $prefix$bind_count-$widget _q_autosuggest_orig_$widget
			;;

		# Completion widget
		completion:*)
			_q_autosuggest_incr_bind_count $widget
			eval "zle -C $prefix$bind_count-${(q)widget} ${${(s.:.)widgets[$widget]}[2,3]}"
			;;
	esac

	# Pass the original widget's name explicitly into the autosuggest
	# function. Use this passed in widget name to call the original
	# widget instead of relying on the $WIDGET variable being set
	# correctly. $WIDGET cannot be trusted because other plugins call
	# zle without the `-w` flag (e.g. `zle self-insert` instead of
	# `zle self-insert -w`).
	eval "_q_autosuggest_bound_${bind_count}_${(q)widget}() {
		_q_autosuggest_widget_$autosuggest_action $prefix$bind_count-${(q)widget} \$@
	}"

	# Create the bound widget
	zle -N -- $widget _q_autosuggest_bound_${bind_count}_$widget
}

# Map all configured widgets to the right autosuggest widgets
_q_autosuggest_bind_widgets() {
	emulate -L zsh

 	local widget
	local ignore_widgets

	ignore_widgets=(
		.\*
		_\*
		${_Q_AUTOSUGGEST_BUILTIN_ACTIONS/#/autosuggest-}
		$Q_AUTOSUGGEST_ORIGINAL_WIDGET_PREFIX\*
		$Q_AUTOSUGGEST_IGNORE_WIDGETS
	)

	# Find every widget we might want to bind and bind it appropriately
	for widget in ${${(f)"$(builtin zle -la)"}:#${(j:|:)~ignore_widgets}}; do
		if [[ -n ${Q_AUTOSUGGEST_CLEAR_WIDGETS[(r)$widget]} ]]; then
			_q_autosuggest_bind_widget $widget clear
		elif [[ -n ${Q_AUTOSUGGEST_ACCEPT_WIDGETS[(r)$widget]} ]]; then
			_q_autosuggest_bind_widget $widget accept
		elif [[ -n ${Q_AUTOSUGGEST_EXECUTE_WIDGETS[(r)$widget]} ]]; then
			_q_autosuggest_bind_widget $widget execute
		elif [[ -n ${Q_AUTOSUGGEST_PARTIAL_ACCEPT_WIDGETS[(r)$widget]} ]]; then
			_q_autosuggest_bind_widget $widget partial_accept
		else
			# Assume any unspecified widget might modify the buffer
			_q_autosuggest_bind_widget $widget modify
		fi
	done
}

# Given the name of an original widget and args, invoke it, if it exists
_q_autosuggest_invoke_original_widget() {
	# Do nothing unless called with at least one arg
	(( $# )) || return 0

	local original_widget_name="$1"

	shift

	if (( ${+widgets[$original_widget_name]} )); then
		zle $original_widget_name -- $@
	fi
}
