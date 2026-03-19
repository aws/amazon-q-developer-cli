"""
V2 build entry point. Supports 'build', 'sign-macos', and 'sign-bun' subcommands.
"""
import argparse
from build_v2 import build, sign_macos, sign_bun, sign_bun_per_arch


class StoreIfNotEmptyAction(argparse.Action):
    def __call__(self, parser, namespace, values, option_string=None):
        if values and len(values) > 0:
            setattr(namespace, self.dest, values)


parser = argparse.ArgumentParser(
    prog="build",
    description="Builds the kiro-cli-chat binary",
)
subparsers = parser.add_subparsers(help="sub-command help", dest="subparser", required=True)

build_subparser = subparsers.add_parser(name="build")
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

sign_macos_subparser = subparsers.add_parser(name="sign-macos")
sign_macos_subparser.add_argument(
    "--binary-path",
    required=True,
    help="Path to the unsigned binary to sign and notarize",
)

sign_bun_subparser = subparsers.add_parser(name="sign-bun")
sign_bun_subparser.add_argument(
    "--branch-name",
    required=True,
    help="Branch name used to construct the S3 upload path",
)
sign_bun_subparser.add_argument(
    "--commit-sha",
    required=True,
    help="Commit SHA used to construct the S3 upload path",
)

sign_bun_per_arch_subparser = subparsers.add_parser(name="sign-bun-per-arch")
sign_bun_per_arch_subparser.add_argument(
    "--branch-name",
    required=True,
    help="Branch name used to construct the S3 upload path",
)
sign_bun_per_arch_subparser.add_argument(
    "--commit-sha",
    required=True,
    help="Commit SHA used to construct the S3 upload path",
)

args = parser.parse_args()

match args.subparser:
    case "build":
        build(
            release=not args.not_release,
            stage_name=args.stage_name,
            run_lints=not args.skip_lints,
            run_test=not args.skip_tests,
        )
    case "sign-macos":
        sign_macos(binary_path=args.binary_path)
    case "sign-bun":
        sign_bun(branch_name=args.branch_name, commit_sha=args.commit_sha)
    case "sign-bun-per-arch":
        sign_bun_per_arch(branch_name=args.branch_name, commit_sha=args.commit_sha)
    case _:
        raise ValueError(f"Unsupported subparser {args.subparser}")
