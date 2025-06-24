mkdir ~/.local/bin | ignore

def pathadd [path: string] {
  if not ($env.PATH | any {|it| $it == $path }) {
    $env.PATH | prepend $path
  } else {
    $env.PATH
  }
}

let-env PATH = pathadd $"($env.HOME)/.local/bin"
let-env PATH = pathadd $"($env.HOME)/.local/bin"

if "Q_NEW_SESSION" in $env {
  let-env QTERM_SESSION_ID = $nothing
  let-env Q_TERM = $nothing
  let-env Q_NEW_SESSION = $nothing
}

if "Q_SET_PARENT_CHECK" not-in $env {
  if "Q_PARENT" not-in $env and "Q_SET_PARENT" in $env {
    let-env Q_PARENT = $env.Q_SET_PARENT
    let-env Q_SET_PARENT = $nothing
  }
  let-env Q_SET_PARENT_CHECK = 1
}


let result = (^q _ should-figterm-launch | complete)
let-env SHOULD_QTERM_LAUNCH = $result.exit_code

let should_launch = (
    ("PROCESS_LAUNCHED_BY_Q" not-in $env or ($env.PROCESS_LAUNCHED_BY_Q | str length) == 0)
    and ($env.SHOULD_QTERM_LAUNCH == 0 or
       ($env.SHOULD_QTERM_LAUNCH == 2 and "Q_TERM" not-in $env))
)

if $should_launch {
  let Q_SHELL = (q _ get-shell | complete).stdout
  
  let fig_term_name = "nu (figterm)"
  let figterm_path = if ([$env.HOME ".fig" "bin" $fig_term_name] | path join | path exists) {
    [$env.HOME ".fig" "bin" $fig_term_name] | path join
  } else if (which figterm | length) > 0 {
    which figterm | first | get path
  } else {
    [$env.HOME ".fig" "bin" "figterm"] | path join
  }

  with-env {
    Q_SHELL: $Q_SHELL
  } {
    exec $figterm_path
  }
}
