import itertools
import os
from typing import List, Mapping, Sequence
from rust import cargo_cmd_name, rust_env
from util import isLinux, run_cmd, get_variants, Variant
from const import DESKTOP_FUZZ_PACKAGE_NAME, DESKTOP_PACKAGE_NAME


def run_clippy(
    variants: List[Variant],
    features: Mapping[str, Sequence[str]] | None = None,
    target: str | None = None,
    fail_on_warn: bool = False,
):
    args = [cargo_cmd_name(), "clippy", "--locked", "--workspace", "--exclude", "zbus", "--exclude", "zbus_names"]

    if target:
        args.extend(["--target", target])

    if Variant.FULL not in variants:
        args.extend(["--exclude", DESKTOP_PACKAGE_NAME, "--exclude", DESKTOP_FUZZ_PACKAGE_NAME])

    if features:
        args.extend(
            [
                "--features",
                ",".join(set(itertools.chain.from_iterable(features.values()))),
            ]
        )

    if fail_on_warn:
        args.extend(["--", "-D", "warnings"])

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=False),
        },
    )


def run_cargo_tests(
    variants: List[Variant],
    features: Mapping[str, Sequence[str]] | None = None,
    target: str | None = None,
):
    args = [cargo_cmd_name()]

    args.extend(["build", "--tests", "--locked", "--workspace", "--exclude", DESKTOP_FUZZ_PACKAGE_NAME])

    if target:
        args.extend(["--target", target])

    if Variant.FULL not in variants:
        args.extend(["--exclude", DESKTOP_PACKAGE_NAME])

    if features:
        args.extend(
            [
                "--features",
                ",".join(set(itertools.chain.from_iterable(features.values()))),
            ]
        )

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=False),
        },
    )

    args = [cargo_cmd_name()]

    # Run all lib, bin, and integration tests. Required to exclude running doc tests.
    args.extend(
        ["test", "--locked", "--workspace", "--lib", "--bins", "--test", "*", "--exclude", DESKTOP_FUZZ_PACKAGE_NAME]
    )

    if target:
        args.extend(["--target", target])

    # disable desktop tests for now
    if isLinux():
        args.extend(["--exclude", DESKTOP_PACKAGE_NAME])

    if features:
        args.extend(
            [
                "--features",
                ",".join(set(itertools.chain.from_iterable(features.values()))),
            ]
        )

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=False),
        },
    )


def lint_install_sh():
    run_cmd(["shellcheck", "bundle/linux/install.sh"])


def all_tests(clippy_fail_on_warn: bool):
    variants = get_variants()
    lint_install_sh()
    run_cargo_tests(variants=variants)
    run_clippy(variants=variants, fail_on_warn=clippy_fail_on_warn)
