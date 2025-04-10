#!/usr/bin/env bash

# Fig onboarding shell script.
# Based somewhat on oh my zshell https://github.com/ohmyzsh/ohmyzsh/blob/master/tools/install.sh
set -e

# Force current process to be shell, rather than `env`.
cd ~
TTY=$(tty)
q hook prompt $$ "$TTY" 1>/dev/null 2>&1 

# Colors
YELLOW=$(tput setaf 3)
MAGENTA=$(tput setaf 5)

# Weights and decoration.
BOLD=$(tput bold)
UNDERLINE=$(tput smul)
HIGHLIGHT=$(tput smso)
NORMAL=$(tput sgr0)

# Structure.
TAB='   '
SEPARATOR="  \n\n  --\n\n\n"

function fig_osc { printf "\033]697;"; printf "%s" "$@"; printf "\007"; }

START_PROMPT="$(fig_osc StartPrompt)"
END_PROMPT="$(fig_osc EndPrompt)"
NEW_CMD="$(fig_osc NewCmd)"
END_CMD="$(fig_osc PreExec)"

DEFAULT_PROMPT="${START_PROMPT}${TAB}$ ${END_PROMPT}${NEW_CMD}"

function prepare_prompt {
  fig_osc "Dir=%s" "${PWD}"
  fig_osc "Shell=bash"
  fig_osc "PID=%d" "$$"
  fig_osc "TTY=%s" "${TTY}"
}

function reset_prompt {
    (q hook pre-exec $$ "$TTY" 1>/dev/null 2>&1 )
}

print_special() {
  echo "${START_PROMPT}${TAB}" "$@" "${NORMAL}"$'\n'${END_PROMPT}
  reset_prompt
}

press_enter_to_continue() {
  echo "${START_PROMPT}" # new line

  if [[ "$1" != "" ]]; then
    read -n 1 -s -r -p "${TAB}${HIGHLIGHT} $1 ${NORMAL}" pressed_key 
  else
    read -n 1 -s -r -p "${TAB}${HIGHLIGHT} Press enter to continue ${NORMAL}" pressed_key 
  fi
  printf "%s" "${END_PROMPT}"

  while true; do
    # ie if pressed_key = enter
    if [[ "$pressed_key" == "" ]]; then
      echo # new line
      echo # new line
      break
    else 
      read -n 1 -s -r pressed_key
    fi
  done
}

# In case user quits script
exit_script_nice() {

clear 
cat <<EOF

  ${BOLD}${UNDERLINE}Amazon Q's onboarding was quit${NORMAL}
  
  You can redo this onboarding any time. Just run ${BOLD}${MAGENTA}fig onboarding${NORMAL}
   

  Have an issue? Run ${BOLD}${MAGENTA}fig doctor${NORMAL}
  Have feedback? Email ${UNDERLINE}hello@fig.io${NORMAL}


EOF

  trap - SIGINT SIGTERM SIGQUIT # clear the trap
  exit 1
}

# If the user does ctrl + c, run the exit_script function
trap exit_script_nice SIGINT SIGTERM SIGQUIT

### Core Script ###
clear

# Make absolutely sure that settings listener has been launched!
(fig settings init 1>/dev/null 2>&1)

# Done using http://patorjk.com/software/taag/#p=testall&f=Graffiti&t=fig
# Font name = ANSI Shadow
cat <<'EOF'


   ███████╗██╗ ██████╗ 
   ██╔════╝██║██╔════╝ 
   █████╗  ██║██║  ███╗
   ██╔══╝  ██║██║   ██║
   ██║     ██║╚██████╔╝
   ╚═╝     ╚═╝ ╚═════╝  ....is now installed!


EOF


## you can also use <<-'EOF' to strip tab character from start of each line
cat <<EOF 
   Hey! Welcome to ${MAGENTA}${BOLD}Amazon Q${NORMAL}.

   This quick walkthrough will show you how Amazon Q works.



   Want to quit? Hit ${BOLD}ctrl + c${NORMAL}

EOF

press_enter_to_continue

clear

cat <<EOF
   
   ${BOLD}${MAGENTA}Amazon Q${NORMAL} suggests commands, options, and arguments as you type.

   ${BOLD}Autocomplete Basics${NORMAL}

     * To filter: just start typing
     * To navigate: use the ${BOLD}↓${NORMAL} & ${BOLD}↑${NORMAL} arrow keys
     * To select: hit ${BOLD}enter${NORMAL} or ${BOLD}tab${NORMAL}
     * To hide: press ${BOLD}esc${NORMAL}, or scroll ${BOLD}↑${NORMAL} past the top suggestion to shell history

EOF

press_enter_to_continue
clear

(q hook init $$ "$TTY" 1>/dev/null 2>&1)
cat <<EOF

   ${BOLD}Example${NORMAL}
   Try typing ${BOLD}cd${NORMAL} then space. Autocomplete will suggest the folders in your
   home directory.

   
   ${BOLD}To Continue...${NORMAL}
   cd into the "${BOLD}.fig/${NORMAL}" folder

EOF

prepare_prompt

while true; do
  input=""

  read -r -e -p "$DEFAULT_PROMPT" input
  echo "$END_CMD" # New line after output
  reset_prompt
  case "${input}" in
    cd*)
      cd ~/.fig
      print_special "${BOLD}Awesome!${NORMAL}"
      echo
      print_special "${UNDERLINE}Quick Tip${NORMAL}: Selecting a suggestion with a ${BOLD}🟥 red icon${NORMAL} and ${BOLD}↪${NORMAL} symbol 
              will immediately execute a command"
      press_enter_to_continue
      break
      ;;
    "continue") break ;;
    "c") break ;;
    "") print_special "Type ${BOLD}cd .fig/${NORMAL} to continue" ;;
    *)
      print_special "${YELLOW}Whoops. Looks like you tried something other than cd."
      print_special "Type ${BOLD}cd .fig/${NORMAL} to continue"
      ;;
  esac
done

(q hook init $$ "$TTY" 1>/dev/null 2>&1)
clear 
cat <<EOF

   ${BOLD}Another Example${NORMAL}
   Q can insert text and move your cursor around.

   ${BOLD}To Continue...${NORMAL}

   Run ${BOLD}git commit -m 'hello'${NORMAL}

   
   (Don't worry, this will ${BOLD}not${NORMAL} actually run the git command)

EOF

prepare_prompt
while true; do
  input=""
  read -r -e -p "$DEFAULT_PROMPT" input
  printf "%s" "$END_CMD"
  echo # New line after output
  case "${input}" in
    "git commit"*)
      reset_prompt
      print_special "${BOLD}Nice work!${NORMAL}"
      press_enter_to_continue
      reset_prompt
      break
      ;;
    "continue") break ;;
    "c") break ;;
    "")
      print_special "Try running ${BOLD}git commit -m 'hello'${NORMAL} to continue. Otherwise, just type ${BOLD}continue"
      ;;
    *)
      print_special "${YELLOW}Whoops. Looks like you tried something other than ${BOLD}git commit${NORMAL}."
      print_special "Try running ${BOLD}git commit -m 'hello'${NORMAL} to continue. Otherwise, just type ${BOLD}continue"
      ;;
  esac
done

clear 

(q hook init $$ "$TTY" 1>/dev/null 2>&1)
cat <<EOF
   
   ${BOLD}Last Step: The ${MAGENTA}Fig${NORMAL} ${BOLD}CLI${NORMAL}

   fig              your home for everything Fig
   fig doctor       check if Fig is properly configured
   fig settings     update preferences (keybindings, UI, and more)
   fig tweet        share your terminal set up with the world!
   fig update       check for updates
   fig --help       a summary of Fig commands with examples

EOF

press_enter_to_continue 'Press enter to finish'
echo
echo

# Done using http://patorjk.com/software/taag/#p=testall&f=Graffiti&t=fig
# Font name = Ivrit
clear
# Prompt user to restart terminal after running Fig doctor
q _ local-state doctor.prompt-restart-terminal true 1>/dev/null 2>&1
cat <<EOF

   ${BOLD}One last thing...${NORMAL}

   Run ${MAGENTA}${BOLD}fig doctor${NORMAL} right now. 
   This checks for common bugs and fixes them!

EOF
