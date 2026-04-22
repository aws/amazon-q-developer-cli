import argparse
import pathlib
from build import build, sign_bun_per_arch, merge_darwin_universal


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
build_subparser.add_argument(
    "--skip-autodocs-embeddings",
    action="store_true",
    help="Skip generating documentation embeddings",
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

merge_darwin_subparser = subparsers.add_parser(name="merge-darwin")
merge_darwin_subparser.add_argument(
    "--x86_64-binary",
    required=True,
    type=pathlib.Path,
    help="Path to the pre-built x86_64-apple-darwin binary",
)
merge_darwin_subparser.add_argument(
    "--aarch64-binary",
    required=True,
    type=pathlib.Path,
    help="Path to the pre-built aarch64-apple-darwin binary",
)

args = parser.parse_args()

match args.subparser:
    case "build":
        build(
            release=not args.not_release,
            stage_name=args.stage_name,
            run_lints=not args.skip_lints,
            run_test=not args.skip_tests,
            run_autodocs_embeddings=not args.skip_autodocs_embeddings,
        )
    case "sign-bun-per-arch":
        sign_bun_per_arch(branch_name=args.branch_name, commit_sha=args.commit_sha)
    case "merge-darwin":
        x86_64_bin = getattr(args, "x86_64_binary")
        aarch64_bin = getattr(args, "aarch64_binary")
        merge_darwin_universal(x86_64_binary=x86_64_bin, aarch64_binary=aarch64_bin)
    case _:
        raise ValueError(f"Unsupported subparser {args.subparser}")
