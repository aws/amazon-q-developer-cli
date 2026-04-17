from functools import cache
import os
import shlex
import stat
import subprocess
import pathlib
import platform
from typing import Mapping, Sequence

DEBUG = "\033[94;1m"
INFO = "\033[92;1m"
WARN = "\033[93;1m"
FAIL = "\033[91;1m"
ENDC = "\033[0m"


@cache
def isCi() -> bool:
    return os.environ.get("CI") is not None


@cache
def isDarwin() -> bool:
    return platform.system() == "Darwin"


@cache
def isLinux() -> bool:
    return platform.system() == "Linux"


@cache
def isWindows() -> bool:
    return platform.system() == "Windows"


@cache
def isMusl() -> bool:
    return bool(os.environ.get("AMAZON_Q_BUILD_MUSL"))


@cache
def isCrossCompiling() -> bool:
    return bool(os.environ.get("AMAZON_Q_BUILD_TARGET_TRIPLE"))


def log(*value: object, title: str, color: str | None):
    if isCi() or color is None:
        print(f"{title}:", *value, flush=True)
    else:
        print(f"{color}{title}:{ENDC}", *value, flush=True)


def debug(*value: object):
    log(*value, title="DEBUG", color=DEBUG)


def info(*value: object):
    log(*value, title="INFO", color=INFO)


def warn(*value: object):
    log(*value, title="WARN", color=WARN)


def fail(*value: object):
    log(*value, title="FAIL", color=FAIL)


Args = Sequence[str | os.PathLike]
Env = Mapping[str, str | os.PathLike]
Cwd = str | os.PathLike


def run_cmd(args: Args, env: Env | None = None, cwd: Cwd | None = None, check: bool = True):
    args_str = [str(arg) for arg in args]
    print(f"+ {shlex.join(args_str)}")
    subprocess.run(args, env=env, cwd=cwd, check=check)


def run_cmd_output(
    args: Args,
    env: Env | None = None,
    cwd: Cwd | None = None,
) -> str:
    args_str = [str(arg) for arg in args]
    print(f"+ {shlex.join(args_str)}")
    res = subprocess.run(args, env=env, cwd=cwd, check=True, stdout=subprocess.PIPE)
    return res.stdout.decode("utf-8")


def run_cmd_status(
    args: Args,
    env: Env | None = None,
    cwd: Cwd | None = None,
) -> int:
    res = subprocess.run(args, env=env, cwd=cwd)
    return res.returncode


def set_executable(path: pathlib.Path):
    st = os.stat(path)
    os.chmod(path, st.st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
