from const import DESKTOP_PACKAGE_NAME
from rust import cargo_cmd_name
from util import isLinux, run_cmd


def run_doc():
    doc_args = [cargo_cmd_name(), "doc", "--no-deps", "--workspace"]
    if isLinux():
        doc_args.extend(["--exclude", DESKTOP_PACKAGE_NAME])
    run_cmd(doc_args)
