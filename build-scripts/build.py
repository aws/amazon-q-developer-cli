import os
import json
import datetime
import pathlib
import shutil
import sys
from typing import Dict, Sequence
from util import IS_DARWIN, IS_LINUX, n, run_cmd, run_cmd_output, info
from signing import SigningData, SigningType, rebundle_dmg, sign_file, notarize_file


OUTDIR = pathlib.Path("build").absolute()


def build_npm_packages() -> Dict[str, pathlib.Path]:
    run_cmd(["pnpm", "install", "--frozen-lockfile"])
    run_cmd(["pnpm", "build"])

    # copy to output
    dashboard_path = OUTDIR / "dashboard"
    shutil.rmtree(dashboard_path, ignore_errors=True)
    shutil.copytree("apps/dashboard/dist", dashboard_path)

    autocomplete_path = OUTDIR / "autocomplete"
    shutil.rmtree(autocomplete_path, ignore_errors=True)
    shutil.copytree("apps/autocomplete/dist", autocomplete_path)

    return {"dashboard": dashboard_path, "autocomplete": autocomplete_path}


def rust_env(linker=None) -> Dict[str, str]:
    rustflags = [
        "-C force-frame-pointers=yes",
    ]

    if linker:
        rustflags.append(f"-C link-arg=-fuse-ld={linker}")

    if IS_LINUX:
        rustflags.append("-C link-arg=-Wl,--compress-debug-sections=zlib")

    env = {
        "CARGO_INCREMENTAL": "0",
        "CARGO_PROFILE_RELEASE_LTO": "thin",
        "RUSTFLAGS": " ".join(rustflags),
        "CARGO_NET_GIT_FETCH_WITH_CLI": "true",
    }

    if IS_DARWIN:
        env["MACOSX_DEPLOYMENT_TARGET"] = "10.13"

    return env


def build_cargo_bin(
    package: str, output_name: str | None = None, features: Sequence[str] | None = None
) -> pathlib.Path:
    if IS_DARWIN:
        targets = ["x86_64-apple-darwin", "aarch64-apple-darwin"]
    elif IS_LINUX:
        targets = ["x86_64-unknown-linux-gnu"]
    else:
        raise ValueError("Unsupported platform")

    args = ["cargo", "build", "--release", "--locked", "--package", package]
    for target in targets:
        args.extend(["--target", target])

    if features:
        args.extend(["--features", ",".join(features)])

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(),
        },
    )

    # create "univeral" binary for macos
    if IS_DARWIN:
        outpath = OUTDIR / f"{output_name or package}-universal-apple-darwin"

        args = [
            "lipo",
            "-create",
            "-output",
            outpath,
        ]
        for target in targets:
            args.extend(
                [
                    f"target/{target}/release/{package}",
                ]
            )
        run_cmd(args)
        return outpath
    else:
        # assumes linux
        target_path = pathlib.Path("target/x86_64-unknown-linux-gnu/release") / package
        out_path = OUTDIR / (output_name or package)
        shutil.copy2(target_path, out_path)
        return out_path


def run_cargo_tests(features: Sequence[str] | None = None):
    args = ["cargo", "test", "--release", "--locked"]
    if features:
        args.extend(["--features", ",".join(features)])

    run_cmd(
        args,
        env={
            **os.environ,
            **rust_env(),
            "RUST_BACKTRACE": "1",
        },
    )


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
        if pkg["name"] == "fig_desktop":
            return pkg["version"]
    raise ValueError("Version not found")


def gen_manifest() -> str:
    variant = "full"

    dc_output = run_cmd_output(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
        ]
    )
    dc = json.loads(dc_output)["metadata"]["channel"]

    return json.dumps(
        {
            "managed_by": "dmg",
            "packaged_at": datetime.datetime.now().isoformat(),
            "packaged_by": "amazon",
            "variant": variant,
            "version": version(),
            "kind": "dmg",
            "default_channel": dc,
        }
    )


def build_macos_ime(signing_data: SigningData | None) -> pathlib.Path:
    fig_input_method_bin = build_cargo_bin("fig_input_method")
    input_method_app = pathlib.Path("build/CodeWhispererInputMethod.app")

    (input_method_app / "Contents/MacOS").mkdir(parents=True, exist_ok=True)

    shutil.copy2(
        fig_input_method_bin,
        input_method_app / "Contents/MacOS/fig_input_method",
    )
    shutil.copy2(
        "fig_input_method/Info.plist",
        input_method_app / "Contents/Info.plist",
    )
    shutil.copytree(
        "fig_input_method/resources",
        input_method_app / "Contents/Resources",
        dirs_exist_ok=True,
    )

    if signing_data:
        info("Signing macos ime")
        sign_file(input_method_app, SigningType.IME, signing_data)
        notarize_file(input_method_app, signing_data)

    return input_method_app


def tauri_config(
    cw_cli_path: pathlib.Path, cwterm_path: pathlib.Path, target: str
) -> str:
    config = {
        "tauri": {
            "bundle": {
                "externalBin": [
                    str(cw_cli_path).removesuffix(f"-{target}"),
                    str(cwterm_path).removesuffix(f"-{target}"),
                ],
                "resources": ["manifest.json"],
            }
        }
    }
    return json.dumps(config)


def build_desktop_app(
    cwterm_path: pathlib.Path,
    cw_cli_path: pathlib.Path,
    npm_packages: Dict[str, pathlib.Path],
    signing_data: SigningData | None,
    features: Sequence[str] | None = None,
) -> pathlib.Path:
    target = "universal-apple-darwin"

    info("Building macos ime")
    ime_app = build_macos_ime(signing_data)

    info("Writing manifest")
    manifest_path = pathlib.Path("fig_desktop/manifest.json")
    manifest_path.write_text(gen_manifest())

    info("Building tauri config")
    tauri_config_path = pathlib.Path("fig_desktop/build-config.json")
    tauri_config_path.write_text(
        tauri_config(cw_cli_path=cw_cli_path, cwterm_path=cwterm_path, target=target)
    )

    info("Building fig_desktop")

    cargo_tauri_args = [
        "cargo-tauri",
        "build",
        "--config",
        "build-config.json",
        "--target",
        target,
    ]

    if features:
        cargo_tauri_args.extend(["--features", ",".join(features)])

    run_cmd(
        cargo_tauri_args,
        cwd="fig_desktop",
        env={**os.environ, **rust_env(), "BUILD_DIR": OUTDIR},
    )

    # clean up
    manifest_path.unlink(missing_ok=True)
    tauri_config_path.unlink(missing_ok=True)

    target_bundle = pathlib.Path(
        f"target/{target}/release/bundle/macos/codewhisperer_desktop.app"
    )
    bundle_path = OUTDIR / "CodeWhisperer.app"
    shutil.rmtree(bundle_path, ignore_errors=True)
    shutil.copytree(target_bundle, bundle_path)

    info_plist_path = bundle_path / "Contents/Info.plist"

    # Change the display name of the app
    run_cmd(
        [
            "defaults",
            "write",
            info_plist_path,
            "CFBundleDisplayName",
            "CodeWhisperer",
        ]
    )
    run_cmd(
        [
            "defaults",
            "write",
            info_plist_path,
            "CFBundleName",
            "CodeWhisperer",
        ]
    )

    # Specifies the app is an "agent app"
    run_cmd(["defaults", "write", info_plist_path, "LSUIElement", "-bool", "TRUE"])

    # Add codewhisperer:// association to bundle
    run_cmd(
        [
            "plutil",
            "-insert",
            "CFBundleURLTypes",
            "-xml",
            """<array>
    <dict>
        <key>CFBundleURLName</key>
        <string>com.amazon.codewhisperer</string>
        <key>CFBundleURLSchemes</key>
        <array>
        <string>codewhisperer</string>
        </array>
    </dict>
    </array>
    """,
            info_plist_path,
        ]
    )

    info("Copying CodeWhispererInputMethod.app into bundle")
    helpers_dir = bundle_path / "Contents/Helpers"
    helpers_dir.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ime_app, helpers_dir.joinpath("CodeWhispererInputMethod.app"))

    info("Grabbing themes")
    theme_repo = OUTDIR / "themes"
    shutil.rmtree(theme_repo, ignore_errors=True)
    run_cmd(["git", "clone", "https://github.com/withfig/themes.git", theme_repo])
    shutil.copytree(theme_repo / "themes", bundle_path / "Contents/Resources/themes")

    for package, path in npm_packages.items():
        info(f"Copying {package} into bundle")
        shutil.copytree(path, bundle_path / "Contents/Resources" / package)

    dmg_spec = {
        "title": "CodeWhisperer",
        "icon": "VolumeIcon.icns",
        "background": "background.png",
        "icon-size": 160,
        "format": "ULFO",
        "window": {"size": {"width": 660, "height": 400}},
        "contents": [
            {"x": 180, "y": 170, "type": "file", "path": str(bundle_path)},
            {"x": 480, "y": 170, "type": "link", "path": "/Applications"},
        ],
    }
    dmg_spec_path = pathlib.Path("bundle/dmg/spec.json")
    dmg_spec_path.write_text(json.dumps(dmg_spec))

    dmg_path = OUTDIR.joinpath("CodeWhisperer.dmg")
    dmg_path.unlink(missing_ok=True)

    run_cmd(["pnpm", "appdmg", dmg_spec_path, dmg_path])

    dmg_spec_path.unlink(missing_ok=True)

    if signing_data:
        sign_and_rebundle_macos(
            app_path=bundle_path, dmg_path=dmg_path, signing_data=signing_data
        )

    return dmg_path


def sign_and_rebundle_macos(
    app_path: pathlib.Path, dmg_path: pathlib.Path, signing_data: SigningData
):
    info("Signing app and dmg")

    # Sign the application
    sign_file(app_path, SigningType.APP, signing_data)

    # Notarize the application
    notarize_file(app_path, signing_data)

    # Rebundle the dmg file with the signed and notarized application
    rebundle_dmg(app_path=app_path, dmg_path=dmg_path)

    # Sign the dmg
    sign_file(dmg_path, SigningType.DMG, signing_data)

    # Notarize the dmg
    notarize_file(dmg_path, signing_data)

    info("Done signing!!")


def linux_bundle(
    cwterm_path: pathlib.Path,
    cw_cli_path: pathlib.Path,
    codewhisperer_desktop_path: pathlib.Path,
    is_headless: bool,
):
    if not is_headless:
        for res in [16, 22, 24, 32, 48, 64, 128, 256, 512]:
            shutil.copy2(
                f"fig_desktop/icons/{res}x{res}.png",
                f"build/usr/share/icons/hicolor/{res}x{res}/apps/fig.png",
            )

    info("Copying bundle files")
    bin_path = pathlib.Path("build/usr/bin")
    bin_path.mkdir(parents=True, exist_ok=True)
    shutil.copy2(cw_cli_path, bin_path)
    shutil.copy2(cwterm_path, bin_path)
    shutil.copytree("bundle/linux/headless", OUTDIR, dirs_exist_ok=True)
    if not is_headless:
        shutil.copytree("bundle/linux/desktop", OUTDIR, dirs_exist_ok=True)
        shutil.copy2(codewhisperer_desktop_path, bin_path)


def generate_sha(path: pathlib.Path) -> pathlib.Path:
    shasum_output = run_cmd_output(["shasum", "-a", "256", path])
    sha = shasum_output.split(" ")[0]
    path = path.with_name(f"{path.name}.sha256")
    path.write_text(sha)
    return path


build_args = json.loads(sys.argv[1] if len(sys.argv) > 1 else "{}")

if (
    n(build_args, "signing_bucket")
    and n(build_args, "signing_queue")
    and n(build_args, "apple_id_secret")
):
    signing_data = SigningData(
        bucket_name=build_args["signing_bucket"],
        aws_account_id=build_args["aws_account_id"],
        notarizing_secret_id=build_args["apple_id_secret"],
        signing_request_queue_name=build_args["signing_queue"],
        signing_role_name=build_args["signing_role_name"],
    )
else:
    signing_data = None

if n(build_args, "gamma"):
    features = ["gamma"]
else:
    features = None

info(f"Cargo features: {features}")
info(f"Signing app: {signing_data is not None}")

OUTDIR.mkdir(parents=True, exist_ok=True)

info("Building npm packages")
npm_packages = build_npm_packages()

info("Running cargo tests")
run_cargo_tests(features=features)

info("Building cw_cli")
cw_cli_path = build_cargo_bin("cw_cli", output_name="cw", features=features)

info("Building figterm")
cwterm_path = build_cargo_bin("figterm", output_name="cwterm", features=features)

if IS_DARWIN:
    info("Building CodeWhisperer.dmg")
    dmg_path = build_desktop_app(
        cw_cli_path=cw_cli_path,
        cwterm_path=cwterm_path,
        npm_packages=npm_packages,
        signing_data=signing_data,
        features=features,
    )

    sha_path = generate_sha(dmg_path)

    if n(build_args, "output_bucket"):
        staging_location = f"s3://{build_args['output_bucket']}/staging/"
        info(f"Build complete, sending to {staging_location}")

        run_cmd(["aws", "s3", "cp", dmg_path, staging_location])
        run_cmd(["aws", "s3", "cp", sha_path, staging_location])
elif IS_LINUX:
    if n(build_args, "output_bucket"):
        staging_location = f"s3://{build_args['output_bucket']}/staging/"
        info(f"Build complete, sending to {staging_location}")

        run_cmd(["aws", "s3", "cp", cw_cli_path, staging_location])
        run_cmd(["aws", "s3", "cp", cwterm_path, staging_location])

    # disabled for now
    # build_cargo_bin(
    #     "fig_desktop", output_name="codewhisperer_desktop", features=features
    # )