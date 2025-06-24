
#--------------------------------------------------------------------#
# Autosuggest Widget Implementations                                 #
#--------------------------------------------------------------------#

# Disable suggestions
_q_autosuggest_disable() {
	typeset -g _Q_AUTOSUGGEST_DISABLED
	_q_autosuggest_clear
}

# Enable suggestions
_q_autosuggest_enable() {
	unset _Q_AUTOSUGGEST_DISABLED

	if (( $#BUFFER )); then
		_q_autosuggest_fetch
	fi
}

# Toggle suggestions (enable/disable)
_q_autosuggest_toggle() {
	if (( ${+_Q_AUTOSUGGEST_DISABLED} )); then
		_q_autosuggest_enable
	else
		_q_autosuggest_disable
	fi
}

# Clear the suggestion
_q_autosuggest_clear() {
	# Remove the suggestion
	unset POSTDISPLAY

	_q_autosuggest_invoke_original_widget $@
}

# Modify the buffer and get a new suggestion
_q_autosuggest_modify() {
	local -i retval

	# Only available in zsh >= 5.4
	local -i KEYS_QUEUED_COUNT

	# Save the contents of the buffer/postdisplay
	local orig_buffer="$BUFFER"
	local orig_postdisplay="$POSTDISPLAY"

	# Clear suggestion while waiting for next one
	unset POSTDISPLAY

	# Original widget may modify the buffer
	_q_autosuggest_invoke_original_widget $@
	retval=$?

	emulate -L zsh

	# Don't fetch a new suggestion if there's more input to be read immediately
	if (( $PENDING > 0 || $KEYS_QUEUED_COUNT > 0 )); then
		POSTDISPLAY="$orig_postdisplay"
		return $retval
	fi

	# Optimize if manually typing in the suggestion or if buffer hasn't changed
	if [[ "$BUFFER" = "$orig_buffer"* && "$orig_postdisplay" = "${BUFFER:$#orig_buffer}"* ]]; then
		POSTDISPLAY="${orig_postdisplay:$(($#BUFFER - $#orig_buffer))}"
		return $retval
	fi

	# Bail out if suggestions are disabled
	if (( ${+_Q_AUTOSUGGEST_DISABLED} )); then
		return $?
	fi

	# Get a new suggestion if the buffer is not empty after modification
	if (( $#BUFFER > 0 )); then
		if [[ -z "$Q_AUTOSUGGEST_BUFFER_MAX_SIZE" ]] || (( $#BUFFER <= $Q_AUTOSUGGEST_BUFFER_MAX_SIZE )); then
			_q_autosuggest_fetch
		fi
	fi

	return $retval
}

# Fetch a new suggestion based on what's currently in the buffer
_q_autosuggest_fetch() {
	if (( ${+Q_AUTOSUGGEST_USE_ASYNC} )); then
		_q_autosuggest_async_request "$BUFFER"
	else
		local suggestion
		_q_autosuggest_fetch_suggestion "$BUFFER"
		_q_autosuggest_suggest "$suggestion"
	fi
}

# Offer a suggestion
_q_autosuggest_suggest() {
	emulate -L zsh

	local suggestion="$1"

	if [[ -n "$suggestion" ]] && (( $#BUFFER )); then
		POSTDISPLAY="${suggestion#$BUFFER}"
	else
		unset POSTDISPLAY
	fi
}

# Accept the entire suggestion
_q_autosuggest_accept() {
	local -i retval max_cursor_pos=$#BUFFER

	# When vicmd keymap is active, the cursor can't move all the way
	# to the end of the buffer
	if [[ "$KEYMAP" = "vicmd" ]]; then
		max_cursor_pos=$((max_cursor_pos - 1))
	fi

	# If we're not in a valid state to accept a suggestion, just run the
	# original widget and bail out
	if (( $CURSOR != $max_cursor_pos || !$#POSTDISPLAY )); then
		_q_autosuggest_invoke_original_widget $@
		return
	fi

	(q _ inline-shell-completion-accept --buffer "$BUFFER" --suggestion "$POSTDISPLAY" > /dev/null 2>&1 &)

	# Only accept if the cursor is at the end of the buffer
	# Add the suggestion to the buffer
	BUFFER="$BUFFER$POSTDISPLAY"

	# Remove the suggestion
	unset POSTDISPLAY

	# Run the original widget before manually moving the cursor so that the
	# cursor movement doesn't make the widget do something unexpected
	_q_autosuggest_invoke_original_widget $@
	retval=$?

	# Move the cursor to the end of the buffer
	if [[ "$KEYMAP" = "vicmd" ]]; then
		CURSOR=$(($#BUFFER - 1))
	else
		CURSOR=$#BUFFER
	fi

	return $retval
}

# Accept the entire suggestion and execute it
_q_autosuggest_execute() {
	# background so we don't block the terminal
	(q _ inline-shell-completion-accept --buffer "$BUFFER" --suggestion "$POSTDISPLAY" > /dev/null 2>&1 &)

	# Add the suggestion to the buffer
	BUFFER="$BUFFER$POSTDISPLAY"

	# Remove the suggestion
	unset POSTDISPLAY

	# Call the original `accept-line` to handle syntax highlighting or
	# other potential custom behavior
	_q_autosuggest_invoke_original_widget "accept-line"
}

# Partially accept the suggestion
_q_autosuggest_partial_accept() {
	local -i retval cursor_loc

	# Save the contents of the buffer so we can restore later if needed
	local original_buffer="$BUFFER"

	# Temporarily accept the suggestion.
	BUFFER="$BUFFER$POSTDISPLAY"

	# Original widget moves the cursor
	_q_autosuggest_invoke_original_widget $@
	retval=$?

	# Normalize cursor location across vi/emacs modes
	cursor_loc=$CURSOR
	if [[ "$KEYMAP" = "vicmd" ]]; then
		cursor_loc=$((cursor_loc + 1))
	fi

	# If we've moved past the end of the original buffer
	if (( $cursor_loc > $#original_buffer )); then
		# Set POSTDISPLAY to text right of the cursor
		POSTDISPLAY="${BUFFER[$(($cursor_loc + 1)),$#BUFFER]}"

		# Clip the buffer at the cursor
		BUFFER="${BUFFER[1,$cursor_loc]}"
	else
		# Restore the original buffer
		BUFFER="$original_buffer"
	fi

	return $retval
}

() {
	typeset -ga _Q_AUTOSUGGEST_BUILTIN_ACTIONS

	_Q_AUTOSUGGEST_BUILTIN_ACTIONS=(
		clear
		fetch
		suggest
		accept
		execute
		enable
		disable
		toggle
	)

	local action
	for action in $_Q_AUTOSUGGEST_BUILTIN_ACTIONS modify partial_accept; do
		eval "_q_autosuggest_widget_$action() {
			local -i retval

			_q_autosuggest_highlight_reset

			_q_autosuggest_$action \$@
			retval=\$?

			_q_autosuggest_highlight_apply

			zle -R

			return \$retval
		}"
	done

	for action in $_Q_AUTOSUGGEST_BUILTIN_ACTIONS; do
		zle -N autosuggest-$action _q_autosuggest_widget_$action
	done
}
