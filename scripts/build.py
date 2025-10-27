import base64
from dataclasses import dataclass
import json
import pathlib
from functools import cache
import os
import shutil
import time
from typing import Any, Mapping, Sequence, List, Optional
from const import APPLE_TEAM_ID, CHAT_BINARY_NAME, CHAT_PACKAGE_NAME
from util import debug, info, isDarwin, isLinux, run_cmd, run_cmd_output, warn
from rust import cargo_cmd_name, rust_env, rust_targets
from importlib import import_module

Args = Sequence[str | os.PathLike]
Env = Mapping[str, str | os.PathLike]
Cwd = str | os.PathLike

BUILD_DIR_RELATIVE = pathlib.Path(os.environ.get("BUILD_DIR") or "build")
BUILD_DIR = BUILD_DIR_RELATIVE.absolute()

CD_SIGNER_REGION = "us-west-2"
SIGNING_API_BASE_URL = "https://api.signer.builder-tools.aws.dev"


@dataclass
class CdSigningData:
    bucket_name: str
    """The bucket hosting signing artifacts accessible by CD Signer."""
    apple_notarizing_secret_arn: str
    """The ARN of the secret containing the Apple ID and password, used during notarization"""
    signing_role_arn: str
    """The ARN of the role used by CD Signer"""


@dataclass
class MacOSBuildOutput:
    chat_path: pathlib.Path
    """The path to the chat binary"""
    chat_zip_path: pathlib.Path
    """The path to the chat binary zipped"""


def run_cargo_tests():
    args = [cargo_cmd_name()]
    args.extend(["test", "--locked", "--package", CHAT_PACKAGE_NAME])
    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=False),
        },
    )


def run_clippy():
    args = [cargo_cmd_name(), "clippy", "--locked", "--package", CHAT_PACKAGE_NAME]
    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=False),
        },
    )


def build_chat_bin(
    release: bool,
    output_name: str | None = None,
    targets: Sequence[str] = [],
):
    package = CHAT_PACKAGE_NAME

    args = [cargo_cmd_name(), "build", "--locked", "--package", package]

    for target in targets:
        args.extend(["--target", target])

    if release:
        args.append("--release")
        target_subdir = "release"
    else:
        target_subdir = "debug"

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(release=release),
        },
    )

    # create "universal" binary for macos
    if isDarwin():
        out_path = BUILD_DIR / f"{output_name or package}-universal-apple-darwin"
        args = [
            "lipo",
            "-create",
            "-output",
            out_path,
        ]
        for target in targets:
            args.append(pathlib.Path("target") / target / target_subdir / package)
        run_cmd(args)
        return out_path
    else:
        # linux does not cross compile arch
        target = targets[0]
        target_path = pathlib.Path("target") / target / target_subdir / package
        out_path = BUILD_DIR / "bin" / f"{(output_name or package)}-{target}"
        out_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(target_path, out_path)
        return out_path


@cache
def get_creds():
    boto3 = import_module("boto3")
    session = boto3.Session()
    credentials = session.get_credentials()
    creds = credentials.get_frozen_credentials()
    return creds


def cd_signer_request(method: str, path: str, data: str | None = None):
    """
    Sends a request to the CD Signer API.
    """
    SigV4Auth = import_module("botocore.auth").SigV4Auth
    AWSRequest = import_module("botocore.awsrequest").AWSRequest
    requests = import_module("requests")

    url = f"{SIGNING_API_BASE_URL}{path}"
    headers = {"Content-Type": "application/json"}
    request = AWSRequest(method=method, url=url, data=data, headers=headers)
    SigV4Auth(get_creds(), "signer-builder-tools", CD_SIGNER_REGION).add_auth(request)

    for i in range(1, 8):
        debug(f"Sending request {method} to {url} with data: {data}")
        response = requests.request(method=method, url=url, headers=dict(request.headers), data=data)
        info(f"CDSigner Request ({url}): {response.status_code}")
        if response.status_code == 429:
            warn(f"Too many requests, backing off for {2**i} seconds")
            time.sleep(2**i)
            continue
        return response

    raise Exception(f"Failed to request {url}")


def cd_signer_create_request(manifest: Any) -> str:
    """
    Sends a POST request to create a new signing request. After creation, we
    need to send another request to start it.
    """
    response = cd_signer_request(
        method="POST",
        path="/signing_requests",
        data=json.dumps({"manifest": manifest}),
    )
    response_json = response.json()
    info(f"Signing request create: {response_json}")
    request_id = response_json["signingRequestId"]
    return request_id


def cd_signer_start_request(request_id: str, source_key: str, destination_key: str, signing_data: CdSigningData):
    """
    Sends a POST request to start the signing process.
    """
    response_text = cd_signer_request(
        method="POST",
        path=f"/signing_requests/{request_id}/start",
        data=json.dumps(
            {
                "iamRole": f"{signing_data.signing_role_arn}",
                "s3Location": {
                    "bucket": signing_data.bucket_name,
                    "sourceKey": source_key,
                    "destinationKey": destination_key,
                },
            }
        ),
    ).text
    info(f"Signing request start: {response_text}")


def cd_signer_status_request(request_id: str):
    response_json = cd_signer_request(
        method="GET",
        path=f"/signing_requests/{request_id}",
    ).json()
    info(f"Signing request status: {response_json}")
    return response_json["signingRequest"]["status"]


def cd_build_signed_package(exe_path: pathlib.Path):
    """
    Creates a tarball `package.tar.gz` with the following structure:
    ```
    package
    ├─ EXECUTABLES_TO_SIGN
    | ├─ kiro-cli-chat
    ```
    """
    # Trying a different format without manifest.yaml and placing EXECUTABLES_TO_SIGN
    # at the root.
    # The docs contain conflicting information, idk what to even do here
    working_dir = BUILD_DIR / "package"
    shutil.rmtree(working_dir, ignore_errors=True)
    (BUILD_DIR / "package" / "EXECUTABLES_TO_SIGN").mkdir(parents=True)

    shutil.copy2(exe_path, working_dir / "EXECUTABLES_TO_SIGN" / exe_path.name)
    exe_path.unlink()

    run_cmd(["gtar", "-czf", "artifact.gz", "EXECUTABLES_TO_SIGN"], cwd=working_dir)
    run_cmd(
        ["gtar", "-czf", BUILD_DIR / "package.tar.gz", "artifact.gz"],
        cwd=working_dir,
    )

    return BUILD_DIR / "package.tar.gz"


def manifest(
    identifier: str,
):
    """
    Returns the manifest arguments required when creating a new CD Signer request.
    """
    return {
        "type": "app",
        "os": "osx",
        "name": "EXECUTABLES_TO_SIGN",
        "outputs": [{"label": "macos", "path": "EXECUTABLES_TO_SIGN"}],
        "app": {
            "identifier": identifier,
            "signing_requirements": {
                "certificate_type": "developerIDAppDistribution",
                "app_id_prefix": APPLE_TEAM_ID,
            },
        },
    }


def sign_executable(signing_data: CdSigningData, exe_path: pathlib.Path) -> pathlib.Path:
    """
    Signs an executable with CD Signer.

    Returns:
        The path to the signed executable
    """
    name = exe_path.name
    info(f"Signing {name}")

    info("Packaging...")
    package_path = cd_build_signed_package(exe_path)

    info("Uploading...")
    run_cmd(["aws", "s3", "rm", "--recursive", f"s3://{signing_data.bucket_name}/signed"])
    run_cmd(["aws", "s3", "rm", "--recursive", f"s3://{signing_data.bucket_name}/pre-signed"])
    run_cmd(["aws", "s3", "cp", package_path, f"s3://{signing_data.bucket_name}/pre-signed/package.tar.gz"])

    info("Sending request...")
    request_id = cd_signer_create_request(manifest("com.amazon.codewhisperer"))
    cd_signer_start_request(
        request_id=request_id,
        source_key="pre-signed/package.tar.gz",
        destination_key="signed/signed.zip",
        signing_data=signing_data,
    )

    max_duration = 180
    end_time = time.time() + max_duration
    i = 1
    while True:
        info(f"Checking for signed package attempt #{i}")
        status = cd_signer_status_request(request_id)
        info(f"Package has status: {status}")

        match status:
            case "success":
                break
            case "created" | "processing" | "inProgress":
                pass
            case "failure":
                raise RuntimeError("Signing request failed")
            case _:
                warn(f"Unexpected status, ignoring: {status}")

        if time.time() >= end_time:
            raise RuntimeError("Signed package did not appear, check signer logs")
        time.sleep(2)
        i += 1

    info("Signed!")

    # CD Signer should return the signed executable in a zip file containing the structure:
    # "Payload/EXECUTABLES_TO_SIGN/{executable name}".
    info("Downloading...")

    # Create a new directory for unzipping the signed executable.
    zip_dl_path = BUILD_DIR / pathlib.Path("signed.zip")
    run_cmd(["aws", "s3", "cp", f"s3://{signing_data.bucket_name}/signed/signed.zip", zip_dl_path])
    payload_path = BUILD_DIR / "signed"
    shutil.rmtree(payload_path, ignore_errors=True)
    run_cmd(["unzip", zip_dl_path, "-d", payload_path])
    zip_dl_path.unlink()
    signed_exe_path = BUILD_DIR / "signed" / "Payload" / "EXECUTABLES_TO_SIGN" / name
    # Verify that the exe is signed
    run_cmd(["codesign", "--verify", "--verbose=4", signed_exe_path])
    return signed_exe_path


def notarize_executable(signing_data: CdSigningData, exe_path: pathlib.Path):
    """
    Submits an executable to Apple notary service.
    """
    # Load the Apple id and password from secrets manager.
    secret_id = signing_data.apple_notarizing_secret_arn
    secret_region = parse_region_from_arn(signing_data.apple_notarizing_secret_arn)
    info(f"Loading secretmanager value: {secret_id}")
    secret_value = run_cmd_output(
        ["aws", "--region", secret_region, "secretsmanager", "get-secret-value", "--secret-id", secret_id]
    )
    secret_string = json.loads(secret_value)["SecretString"]
    secrets = json.loads(secret_string)

    # Submit the exe to Apple notary service. It must be zipped first.
    info(f"Submitting {exe_path} to Apple notary service")
    zip_path = BUILD_DIR / f"{exe_path.name}.zip"
    zip_path.unlink(missing_ok=True)
    run_cmd(["zip", "-j", zip_path, exe_path], cwd=BUILD_DIR)
    submit_res = run_cmd_output(
        [
            "xcrun",
            "notarytool",
            "submit",
            zip_path,
            "--team-id",
            APPLE_TEAM_ID,
            "--apple-id",
            secrets["appleId"],
            "--password",
            secrets["appleIdPassword"],
            "--wait",
            "-f",
            "json",
        ]
    )
    debug(f"Notary service response: {submit_res}")

    # Confirm notarization succeeded.
    assert json.loads(submit_res)["status"] == "Accepted"

    # Cleanup
    zip_path.unlink()


def sign_and_notarize(signing_data: CdSigningData, chat_path: pathlib.Path) -> pathlib.Path:
    """
    Signs an executable with CD Signer, and verifies it with Apple notary service.

    Returns:
        The path to the signed executable.
    """
    # First, sign the application
    chat_path = sign_executable(signing_data, chat_path)

    # Next, notarize the application
    notarize_executable(signing_data, chat_path)

    return chat_path


def build_macos(chat_path: pathlib.Path, signing_data: CdSigningData | None):
    """
    Creates a kiro-cli-chat.zip under the build directory.
    """
    chat_dst = BUILD_DIR / CHAT_BINARY_NAME
    chat_dst.unlink(missing_ok=True)
    shutil.copy2(chat_path, chat_dst)

    if signing_data:
        chat_dst = sign_and_notarize(signing_data, chat_dst)

    zip_path = BUILD_DIR / f"{CHAT_BINARY_NAME}.zip"
    zip_path.unlink(missing_ok=True)

    info(f"Creating zip output to {zip_path}")
    run_cmd(["zip", "-j", zip_path, chat_dst], cwd=BUILD_DIR)
    generate_sha(zip_path)


class GpgSigner:
    def __init__(self, gpg_id: str, gpg_secret_key: str, gpg_passphrase: str):
        self.gpg_id = gpg_id
        self.gpg_secret_key = gpg_secret_key
        self.gpg_passphrase = gpg_passphrase

        self.gpg_home = pathlib.Path.home() / ".gnupg-tmp"
        self.gpg_home.mkdir(parents=True, exist_ok=True, mode=0o700)

        # write gpg secret key to file
        self.gpg_secret_key_path = self.gpg_home / "gpg_secret"
        self.gpg_secret_key_path.write_bytes(base64.b64decode(gpg_secret_key))

        self.gpg_passphrase_path = self.gpg_home / "gpg_pass"
        self.gpg_passphrase_path.write_text(gpg_passphrase)

        run_cmd(["gpg", "--version"])

        info("Importing GPG key")
        run_cmd(["gpg", "--list-keys"], env=self.gpg_env())
        run_cmd(
            ["gpg", *self.sign_args(), "--allow-secret-key-import", "--import", self.gpg_secret_key_path],
            env=self.gpg_env(),
        )
        run_cmd(["gpg", "--list-keys"], env=self.gpg_env())

    def gpg_env(self) -> Env:
        return {**os.environ, "GNUPGHOME": self.gpg_home}

    def sign_args(self) -> Args:
        return [
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--no-tty",
            "--yes",
            "--passphrase-file",
            self.gpg_passphrase_path,
        ]

    def sign_file(self, path: pathlib.Path) -> List[pathlib.Path]:
        info(f"Signing {path.name}")
        run_cmd(
            ["gpg", "--detach-sign", *self.sign_args(), "--local-user", self.gpg_id, path],
            env=self.gpg_env(),
        )
        run_cmd(
            ["gpg", "--detach-sign", *self.sign_args(), "--armor", "--local-user", self.gpg_id, path],
            env=self.gpg_env(),
        )
        return [path.with_suffix(f"{path.suffix}.asc"), path.with_suffix(f"{path.suffix}.sig")]

    def clean(self):
        info("Cleaning gpg keys")
        shutil.rmtree(self.gpg_home, ignore_errors=True)


def get_secretmanager_json(secret_id: str, secret_region: str):
    info(f"Loading secretmanager value: {secret_id}")
    secret_value = run_cmd_output(
        ["aws", "--region", secret_region, "secretsmanager", "get-secret-value", "--secret-id", secret_id]
    )
    secret_string = json.loads(secret_value)["SecretString"]
    return json.loads(secret_string)


def load_gpg_signer() -> Optional[GpgSigner]:
    if gpg_id := os.getenv("TEST_PGP_ID"):
        gpg_secret_key = os.getenv("TEST_PGP_SECRET_KEY")
        gpg_passphrase = os.getenv("TEST_PGP_PASSPHRASE")
        if gpg_secret_key is not None and gpg_passphrase is not None:
            info("Using test pgp key", gpg_id)
            return GpgSigner(gpg_id=gpg_id, gpg_secret_key=gpg_secret_key, gpg_passphrase=gpg_passphrase)

    pgp_secret_arn = os.getenv("SIGNING_PGP_KEY_SECRET_ARN")
    info(f"SIGNING_PGP_KEY_SECRET_ARN: {pgp_secret_arn}")
    if pgp_secret_arn:
        pgp_secret_region = parse_region_from_arn(pgp_secret_arn)
        gpg_secret_json = get_secretmanager_json(pgp_secret_arn, pgp_secret_region)
        gpg_id = gpg_secret_json["gpg_id"]
        gpg_secret_key = gpg_secret_json["gpg_secret_key"]
        gpg_passphrase = gpg_secret_json["gpg_passphrase"]
        return GpgSigner(gpg_id=gpg_id, gpg_secret_key=gpg_secret_key, gpg_passphrase=gpg_passphrase)
    else:
        return None


def parse_region_from_arn(arn: str) -> str:
    # ARN format: arn:partition:service:region:account-id:resource-type/resource-id
    # Check if we have enough parts and the ARN starts with "arn:"
    parts = arn.split(":")
    if len(parts) >= 4:
        return parts[3]

    return ""


def generate_sha(path: pathlib.Path) -> pathlib.Path:
    if isDarwin():
        shasum_output = run_cmd_output(["shasum", "-a", "256", path])
    elif isLinux():
        shasum_output = run_cmd_output(["sha256sum", path])
    else:
        raise Exception("Unsupported platform")

    sha = shasum_output.split(" ")[0]
    path = path.with_name(f"{path.name}.sha256")
    path.write_text(sha)
    info(f"Wrote sha256sum to {path}:", sha)
    return path


def build_linux(chat_path: pathlib.Path, signer: GpgSigner | None):
    """
    Creates kiro-cli-chat.tar.gz and kiro-cli-chat.zip archives under `BUILD_DIR`.
    """
    chat_dst = BUILD_DIR / CHAT_BINARY_NAME
    chat_dst.unlink(missing_ok=True)
    shutil.copy2(chat_path, chat_dst)

    tar_gz_path = BUILD_DIR / f"{CHAT_BINARY_NAME}.tar.gz"
    tar_gz_path.unlink(missing_ok=True)
    info(f"Creating tar output to {tar_gz_path}")
    run_cmd(["tar", "-czf", tar_gz_path, "-C", BUILD_DIR, chat_dst.name], cwd=BUILD_DIR)
    generate_sha(tar_gz_path)
    if signer:
        signer.sign_file(tar_gz_path)

    zip_path = BUILD_DIR / f"{CHAT_BINARY_NAME}.zip"
    zip_path.unlink(missing_ok=True)
    info(f"Creating zip output to {zip_path}")
    run_cmd(["zip", "-j", zip_path, chat_dst], cwd=BUILD_DIR)
    generate_sha(zip_path)
    if signer:
        signer.sign_file(zip_path)

    # clean up
    if signer:
        signer.clean()


def build(
    release: bool,
    stage_name: str | None = None,
    run_lints: bool = True,
    run_test: bool = True,
):
    BUILD_DIR.mkdir(exist_ok=True)

    disable_signing = os.environ.get("DISABLE_SIGNING")

    gpg_signer = load_gpg_signer() if not disable_signing and isLinux() else None
    signing_role_arn = os.environ.get("SIGNING_ROLE_ARN")
    signing_bucket_name = os.environ.get("SIGNING_BUCKET_NAME")
    signing_apple_notarizing_secret_arn = os.environ.get("SIGNING_APPLE_NOTARIZING_SECRET_ARN")
    if (
        not disable_signing
        and isDarwin()
        and signing_role_arn
        and signing_bucket_name
        and signing_apple_notarizing_secret_arn
    ):
        signing_data = CdSigningData(
            bucket_name=signing_bucket_name,
            apple_notarizing_secret_arn=signing_apple_notarizing_secret_arn,
            signing_role_arn=signing_role_arn,
        )
    else:
        signing_data = None

    match stage_name:
        case "prod" | None:
            info("Building for prod")
        case "gamma":
            info("Building for gamma")
        case _:
            raise ValueError(f"Unknown stage name: {stage_name}")

    targets = rust_targets()

    info(f"Release: {release}")
    info(f"Targets: {targets}")
    info(f"Signing app: {signing_data is not None or gpg_signer is not None}")

    if run_test:
        info("Running cargo tests")
        run_cargo_tests()

    if run_lints:
        info("Running cargo clippy")
        run_clippy()

    info("Building", CHAT_PACKAGE_NAME)
    chat_path = build_chat_bin(
        release=release,
        output_name=CHAT_BINARY_NAME,
        targets=targets,
    )

    if isDarwin():
        build_macos(chat_path, signing_data)
    else:
        build_linux(chat_path, gpg_signer)
