import argparse
import os
from pathlib import Path
import shutil
import subprocess
from build import build
from const import CLI_BINARY_NAME, CLI_PACKAGE_NAME, PTY_BINARY_NAME
from doc import run_doc
from rust import cargo_cmd_name, rust_env
from test import all_tests
from util import Variant, get_variants


class StoreIfNotEmptyAction(argparse.Action):
    def __call__(self, parser, namespace, values, option_string=None):
        if values and len(values) > 0:
            setattr(namespace, self.dest, values)


parser = argparse.ArgumentParser(
    prog="build",
    description="Builds the FigIoDesktop application",
)
subparsers = parser.add_subparsers(help="sub-command help", dest="subparser", required=True)

build_subparser = subparsers.add_parser(name="build")
build_subparser.add_argument(
    "--output-bucket",
    action=StoreIfNotEmptyAction,
    help="The name of bucket to store the build artifacts",
)
build_subparser.add_argument(
    "--signing-bucket",
    action=StoreIfNotEmptyAction,
    help="The name of bucket to store the build artifacts",
)
build_subparser.add_argument(
    "--aws-account-id",
    action=StoreIfNotEmptyAction,
    help="The AWS account ID",
)
build_subparser.add_argument(
    "--apple-id-secret",
    action=StoreIfNotEmptyAction,
    help="The Apple ID secret",
)
build_subparser.add_argument(
    "--signing-role-name",
    action=StoreIfNotEmptyAction,
    help="The name of the signing role",
)
build_subparser.add_argument(
    "--stage-name",
    action=StoreIfNotEmptyAction,
    help="The name of the stage",
)
build_subparser.add_argument(
    "--not-release",
    action="store_true",
    help="Build a non-release version",
)
build_subparser.add_argument(
    "--skip-tests",
    action="store_true",
    help="Skip running npm and rust tests",
)
build_subparser.add_argument(
    "--skip-lints",
    action="store_true",
    help="Skip running lints",
)
build_subparser.add_argument("--variant", action=StoreIfNotEmptyAction, help="Variant to build for")

test_subparser = subparsers.add_parser(name="test")
test_subparser.add_argument(
    "--clippy-fail-on-warn",
    action="store_true",
    help="Fail on clippy warnings",
)

# runs CLI with the given arguments
cli_subparser = subparsers.add_parser(name="cli")
cli_subparser.add_argument(
    "args",
    nargs=argparse.REMAINDER,
    help="Arguments to pass to the CLI",
)

install_cli = subparsers.add_parser(name="install-cli")
install_cli.add_argument(
    "--release",
    action="store_true",
    help="Build a release version",
)
install_cli.add_argument(
    "--variant",
    action="store",
    help="variant to build for",
    choices=["minimal", "full"],
)

# run the docs command
subparsers.add_parser(name="doc")

args = parser.parse_args()

match args.subparser:
    case "build":
        if args.variant:
            variants = [Variant[args.variant.upper()]]
        else:
            variants = None
        build(
            release=not args.not_release,
            variants=variants,
            output_bucket=args.output_bucket,
            signing_bucket=args.signing_bucket,
            aws_account_id=args.aws_account_id,
            apple_id_secret=args.apple_id_secret,
            signing_role_name=args.signing_role_name,
            stage_name=args.stage_name,
            run_lints=not args.skip_lints,
            run_test=not args.skip_tests,
        )
    case "test":
        all_tests(
            clippy_fail_on_warn=args.clippy_fail_on_warn,
        )
    case "doc":
        run_doc()
    case "cli":
        subprocess.run(
            [
                cargo_cmd_name(),
                "run",
                f"--bin={CLI_PACKAGE_NAME}",
                *args.args,
            ],
            env={
                **os.environ,
                **rust_env(release=False),
            },
        )
    case "install-cli":
        if args.variant:
            variant = Variant[args.variant.upper()]
        else:
            variant = get_variants()[0]
        output = build(release=args.release, variants=[variant], run_lints=False, run_test=False)[variant]

        pty_path = Path.home() / ".local" / "bin" / PTY_BINARY_NAME
        pty_path.unlink(missing_ok=True)
        shutil.copy2(output.pty_path, pty_path)

        cli_path = Path.home() / ".local" / "bin" / CLI_BINARY_NAME
        cli_path.unlink(missing_ok=True)
        shutil.copy2(output.cli_path, cli_path)
    case _:
        raise ValueError(f"Unsupported subparser {args.subparser}")
