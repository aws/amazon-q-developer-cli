#!/usr/bin/env bash

mkdir -p "${HOME}/.local/bin" > /dev/null 2>&1

# add ~/.local/bin to PATH
if [[ -d "${HOME}/.local/bin" ]] && [[ ":$PATH:" != *":${HOME}/.local/bin:"* ]]; then
  PATH="${PATH:+"$PATH:"}${HOME}/.local/bin"
fi

if [[ -n "${Q_NEW_SESSION:-}" ]]; then
  unset QTERM_SESSION_ID
  unset Q_TERM
  unset Q_NEW_SESSION
fi

if [[ -z "${Q_SET_PARENT_CHECK:-}" ]]; then
  # Load parent from env variables
  if [[ -z "${Q_PARENT:-}" && -n "${Q_SET_PARENT:-}" ]]; then
    export Q_PARENT="$Q_SET_PARENT"
    unset -v Q_SET_PARENT
  fi
  export Q_SET_PARENT_CHECK=1
fi

# 0 = Yes, 1 = No, 2 = Fallback to Q_TERM
if [ -z "${SHOULD_QTERM_LAUNCH:-}" ]; then
  q _ should-figterm-launch 1>/dev/null 2>&1
  SHOULD_QTERM_LAUNCH=$?
fi

# Only launch figterm if current session is not already inside PTY and command exists.
# PWSH var is set when launched by `pwsh -Login`, in which case we don't want to init.
# It is not necessary in Fish.
if   [[ -t 1 ]] \
  && [[ -z "${PROCESS_LAUNCHED_BY_Q:-}" ]] \
  && command -v qterm 1>/dev/null 2>&1 \
  && [[ ("${SHOULD_QTERM_LAUNCH}" -eq 0) || (("${SHOULD_QTERM_LAUNCH}" -eq 2) && (-z "${Q_TERM:-}" || (-z "${Q_TERM_TMUX:-}" && -n "${TMUX:-}"))) ]]
then
  # Pty module sets Q_TERM or Q_TERM_TMUX to avoid running twice.
  if [ -z "${Q_SHELL:-}" ]; then
    Q_SHELL=$(q _ get-shell)
  fi
  Q_IS_LOGIN_SHELL="${Q_IS_LOGIN_SHELL:='0'}"

  # shellcheck disable=SC2030
  if ([[ -n "${BASH:-}" ]] && shopt -q login_shell) \
    || [[ -n "${ZSH_NAME:-}" && -o login ]]; then
    Q_IS_LOGIN_SHELL=1
  fi

  # Do not launch figterm in non-interactive shells (like VSCode Tasks)
  if [[ $- == *i* ]]; then
    Q_TERM_NAME="$(basename "${Q_SHELL}") (qterm)"
    if [[ -z "${Q_TERM_PATH:-}" ]]; then
      if [[ -x "${HOME}/.local/bin/${Q_TERM_NAME}" ]]; then
        Q_TERM_PATH="${HOME}/.local/bin/${Q_TERM_NAME}"
      else
        Q_TERM_PATH="$(command -v qterm || echo "${HOME}/.local/bin/qterm")"
      fi
    fi

    Q_EXECUTION_STRING="${BASH_EXECUTION_STRING:=$ZSH_EXECUTION_STRING}"

    # Get initial text.
    INITIAL_TEXT=""
    # shellcheck disable=SC2031
    if [[ -z "${BASH:-}" || "${BASH_VERSINFO[0]}" -gt "3" ]]; then
      while read -rt 0; do
        if [[ -n "${BASH:-}" ]]; then
          read -r
        fi
        INITIAL_TEXT="${INITIAL_TEXT}${REPLY}\n"
      done
    fi
    Q_EXECUTION_STRING="${Q_EXECUTION_STRING}" Q_START_TEXT="$(printf "%b" "${INITIAL_TEXT}")" Q_SHELL="${Q_SHELL}" Q_IS_LOGIN_SHELL="${Q_IS_LOGIN_SHELL}" exec -a "${Q_TERM_NAME}" "${Q_TERM_PATH}"
  fi
fi
