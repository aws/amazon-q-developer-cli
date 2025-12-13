from enum import Enum
from functools import cache
import json
import os
import shlex
import stat
import subprocess
import pathlib
import platform
from typing import List, Mapping, Sequence
from const import DESKTOP_PACKAGE_NAME, TAURI_PRODUCT_NAME

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
    return os.environ.get("AMAZON_Q_BUILD_MUSL") is not None


@cache
def version() -> str:
    output = run_cmd_output(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
        ]
    )
    data = json.loads(output)
    for pkg in data["packages"]:
        if pkg["name"] == DESKTOP_PACKAGE_NAME:
            return pkg["version"]
    raise ValueError("Version not found")


@cache
def tauri_product_name() -> str:
    """
        Derived from the `package.productName` configured in the tauri.conf.json file.

    Tauri build output paths replace underscores with dashes.
    """
    return TAURI_PRODUCT_NAME.replace("_", "-")


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


class Variant(Enum):
    FULL = 1
    MINIMAL = 2


@cache
def get_variants() -> List[Variant]:
    match platform.system():
        case "Darwin":
            return [Variant.FULL]
        case "Linux":
            is_ubuntu = "ubuntu" in platform.version().lower()
            if is_ubuntu:
                return [Variant.FULL]
            else:
                return [Variant.MINIMAL]
        case other:
            raise ValueError(f"Unsupported platform {other}")


class Package(Enum):
    DEB = "deb"
    APPIMAGE = "appImage"


def enum_encoder(obj):
    if isinstance(obj, Enum):
        return obj.value
    return obj.__dict__
