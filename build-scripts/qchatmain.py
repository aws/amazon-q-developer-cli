import argparse
from qchatbuild import build


class StoreIfNotEmptyAction(argparse.Action):
    def __call__(self, parser, namespace, values, option_string=None):
        if values and len(values) > 0:
            setattr(namespace, self.dest, values)


parser = argparse.ArgumentParser(
    prog="build",
    description="Builds the qchat binary",
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

args = parser.parse_args()

match args.subparser:
    case "build":
        build(
            release=not args.not_release,
            stage_name=args.stage_name,
            run_lints=not args.skip_lints,
            run_test=not args.skip_tests,
        )
    case _:
        raise ValueError(f"Unsupported subparser {args.subparser}")
