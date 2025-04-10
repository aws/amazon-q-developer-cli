import base64
from functools import cache
import os
import pathlib
from typing import Any, List, Optional
from const import APPLE_TEAM_ID
from manifest import CdSigningType, app_manifest, dmg_manifest, ime_manifest
from util import Args, Env, info, run_cmd, run_cmd_output, warn
import json
import shutil
import time
from importlib import import_module

REGION = "us-west-2"
SIGNING_API_BASE_URL = "https://api.signer.builder-tools.aws.dev"


@cache
def get_creds():
    boto3 = import_module("boto3")
    session = boto3.Session()
    credentials = session.get_credentials()
    creds = credentials.get_frozen_credentials()
    return creds


class CdSigningData:
    bucket_name: str
    notarizing_secret_id: str
    aws_account_id: str
    signing_role_name: str

    def __init__(
        self,
        bucket_name: str,
        notarizing_secret_id: str,
        aws_account_id: str,
        signing_role_name: str,
    ):
        self.bucket_name = bucket_name
        self.notarizing_secret_id = notarizing_secret_id
        self.aws_account_id = aws_account_id
        self.signing_role_name = signing_role_name


def cd_signer_request(method: str, path: str, data: str | None = None):
    SigV4Auth = import_module("botocore.auth").SigV4Auth
    AWSRequest = import_module("botocore.awsrequest").AWSRequest
    requests = import_module("requests")

    url = f"{SIGNING_API_BASE_URL}{path}"
    headers = {"Content-Type": "application/json"}
    request = AWSRequest(method=method, url=url, data=data, headers=headers)
    SigV4Auth(get_creds(), "signer-builder-tools", REGION).add_auth(request)

    for i in range(1, 8):
        response = requests.request(method=method, url=url, headers=dict(request.headers), data=data)
        info(f"CDSigner Request ({url}): {response.status_code}")
        if response.status_code == 429:
            warn(f"Too many requests, backing off for {2 ** i} seconds")
            time.sleep(2**i)
            continue
        return response

    raise Exception(f"Failed to request {url}")


def cd_signer_create_request(manifest: Any) -> str:
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
    response_text = cd_signer_request(
        method="POST",
        path=f"/signing_requests/{request_id}/start",
        data=json.dumps(
            {
                "iamRole": f"arn:aws:iam::{signing_data.aws_account_id}:role/{signing_data.signing_role_name}",
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


def cd_build_signed_package(type: CdSigningType, file_path: pathlib.Path, name: str):
    working_dir = pathlib.Path(f"build-config/signing/{type.value}")
    starting_dir = pathlib.Path.cwd()

    if type == CdSigningType.DMG:
        # Our dmg file names vary by platform, so this is templated in the manifest
        manifest_template_path = working_dir / "manifest.yaml.template"
        manifest_path = working_dir / "manifest.yaml"
        manifest_path.write_text(manifest_template_path.read_text().replace("__NAME__", name))

    if file_path.is_dir():
        shutil.copytree(file_path, working_dir / "artifact" / file_path.name)
        shutil.rmtree(file_path)
    elif file_path.is_file():
        shutil.copy2(file_path, working_dir / "artifact" / file_path.name)
        file_path.unlink()
    else:
        raise Exception(f"Unknown file type: {file_path}")

    run_cmd(["gtar", "-czf", working_dir / "artifact.gz", "-C", working_dir / "artifact", "."])
    run_cmd(
        ["gtar", "-czf", starting_dir / "package.tar.gz", "manifest.yaml", "artifact.gz"],
        cwd=working_dir,
    )
    (working_dir / "artifact.gz").unlink()
    shutil.rmtree(working_dir / "artifact")


# Sign a file with CDSigner
def cd_sign_file(file: pathlib.Path, type: CdSigningType, signing_data: CdSigningData, is_prod: bool):
    name = file.name

    info(f"Signing {name}")

    # CDSigner requires us to build up a tar file in an extremely specific format
    info("Packaging...")
    cd_build_signed_package(type, file, name)

    # Upload package for signing to S3
    info("Uploading...")
    run_cmd(["aws", "s3", "rm", "--recursive", f"s3://{signing_data.bucket_name}/signed"])
    run_cmd(["aws", "s3", "rm", "--recursive", f"s3://{signing_data.bucket_name}/pre-signed"])
    run_cmd(["aws", "s3", "cp", "package.tar.gz", f"s3://{signing_data.bucket_name}/pre-signed/package.tar.gz"])
    pathlib.Path("package.tar.gz").unlink()

    info("Sending request...")

    match type:
        case CdSigningType.APP:
            manifest = app_manifest()
        case CdSigningType.DMG:
            manifest = dmg_manifest(name)
        case CdSigningType.IME:
            manifest = ime_manifest()

    request_id = cd_signer_create_request(manifest)

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
        info(f"Checking for signed package {i}")
        status = cd_signer_status_request(request_id)

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

    info("Downloading...")
    run_cmd(["aws", "s3", "cp", f"s3://{signing_data.bucket_name}/signed/signed.zip", "signed.zip"])
    run_cmd(["unzip", "signed.zip"])

    # find child of Payload
    children = list(pathlib.Path("Payload").iterdir())
    if len(children) != 1:
        raise RuntimeError("Payload directory should have exactly one child")

    child_path = children[0]

    # copy child to the original file location
    if child_path.is_dir():
        shutil.copytree(child_path, file)
    elif child_path.is_file():
        shutil.copy2(child_path, file)
    else:
        raise Exception(f"Unknown file type: {child_path}")

    # clean up
    pathlib.Path("signed.zip").unlink()
    shutil.rmtree("Payload")

    info(f"Signing status of {file}")
    run_cmd(["codesign", "-dv", "--deep", "--strict", file])


def rebundle_dmg(dmg_path: pathlib.Path, app_path: pathlib.Path):
    mounting_path = pathlib.Path("/Volumes") / dmg_path.name.replace(".dmg", "")

    info(f"Rebunding {dmg_path}")

    # Try to unmount a dmg if it is already there
    if mounting_path.is_dir():
        run_cmd(["hdiutil", "detach", mounting_path])

    tempdmg_path = pathlib.Path.home() / "temp.dmg"
    tempdmg_path.unlink(missing_ok=True)

    # Convert the dmg to writable
    run_cmd(["hdiutil", "convert", dmg_path, "-format", "UDRW", "-o", tempdmg_path])

    # Mount the dmg
    run_cmd(["hdiutil", "attach", tempdmg_path])

    # Copy in the new app
    run_cmd(["cp", "-R", app_path, mounting_path])

    # Unmount the dmg
    run_cmd(["hdiutil", "detach", mounting_path])

    # Convert the dmg to zipped, read only - this is the only type that EC will accept!!
    dmg_path.unlink()
    run_cmd(["hdiutil", "convert", tempdmg_path, "-format", "UDZO", "-o", dmg_path])


def apple_notarize_file(file: pathlib.Path, signing_data: CdSigningData):
    name = file.name
    file_type = file.suffix[1:]

    file_to_notarize = file

    if file_type == "app":
        # check the app is ready to be notarized
        # TODO(grant): remove the check=False if this works
        run_cmd(["syspolicy_check", "notary-submission", file], check=False)

        # We can submit dmg files as is, but we have to zip up app files in a specific way
        file_to_notarize = pathlib.Path(f"{name}.zip")
        run_cmd(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", file, file_to_notarize])

    secrets = get_secretmanager_json(signing_data.notarizing_secret_id)

    run_cmd(
        [
            "xcrun",
            "notarytool",
            "submit",
            file_to_notarize,
            "--team-id",
            APPLE_TEAM_ID,
            "--apple-id",
            secrets["appleId"],
            "--password",
            secrets["appleIdPassword"],
            "--wait",
        ]
    )

    run_cmd(["xcrun", "stapler", "staple", file])

    if file_type == "app":
        # Verify notarization for .app
        run_cmd(["spctl", "-a", "-v", file])
        pathlib.Path(file_to_notarize).unlink()

        # check the file is ready to be distributed
        # TODO(grant): remove the check=False if this works
        run_cmd(["syspolicy_check", "distribution", file], check=False)
    else:
        # Verify notarization for .dmg
        run_cmd(["spctl", "-a", "-t", "open", "--context", "context:primary-signature", "-v", file])


def get_secretmanager_json(secret_id: str):
    info(f"Loading secretmanager value: {secret_id}")
    secret_value = run_cmd_output(["aws", "secretsmanager", "get-secret-value", "--secret-id", secret_id])
    secret_string = json.loads(secret_value)["SecretString"]
    return json.loads(secret_string)


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


def load_gpg_signer() -> Optional[GpgSigner]:
    if gpg_id := os.getenv("TEST_PGP_ID"):
        gpg_secret_key = os.getenv("TEST_PGP_SECRET_KEY")
        gpg_passphrase = os.getenv("TEST_PGP_PASSPHRASE")
        if gpg_secret_key is not None and gpg_passphrase is not None:
            info("Using test pgp key", gpg_id)
            return GpgSigner(gpg_id=gpg_id, gpg_secret_key=gpg_secret_key, gpg_passphrase=gpg_passphrase)

    pgp_secret_arn = os.getenv("FIG_IO_DESKTOP_PGP_KEY_ARN")
    info(f"FIG_IO_DESKTOP_PGP_KEY_ARN: {pgp_secret_arn}")
    if pgp_secret_arn:
        gpg_secret_json = get_secretmanager_json(pgp_secret_arn)
        gpg_id = gpg_secret_json["gpg_id"]
        gpg_secret_key = gpg_secret_json["gpg_secret_key"]
        gpg_passphrase = gpg_secret_json["gpg_passphrase"]
        return GpgSigner(gpg_id=gpg_id, gpg_secret_key=gpg_secret_key, gpg_passphrase=gpg_passphrase)
    else:
        return None
