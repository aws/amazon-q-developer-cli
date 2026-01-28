#!/usr/bin/env bash

# Update the internal brazil deps, since we dont use brazil we have to do this
# step manually and commit the changes to the repo.

set -ex

TMP_FOLDER="$(mktemp -d)"

function update_pkg() {
    BRAZIL_NAME="$1"
    BRAZIL_VERSION="$2"
    CRATE_NAME="$3"

    URL="https://prod.artifactbrowser.brazil.aws.dev/api/v1/packages/${BRAZIL_NAME}/versions/${BRAZIL_VERSION}.0/platforms/AL2_x86_64/flavors/DEV.STD.PTHREAD/rust1x/package/${BRAZIL_NAME}-${BRAZIL_VERSION}.0/${CRATE_NAME}-${BRAZIL_VERSION}.crate?download=true"

    # download and extract the package
    mcurl -sSfL "$URL" -o "${TMP_FOLDER}/${CRATE_NAME}.tar.gz"
    tar -xzf "${TMP_FOLDER}/${CRATE_NAME}.tar.gz" -C "${TMP_FOLDER}"

    # move the package to the right place
    rm -rf "crates/${CRATE_NAME}"
    mv "${TMP_FOLDER}/${CRATE_NAME}-${BRAZIL_VERSION}" "crates/${CRATE_NAME}"

    # clean up package
    rm -rf "crates/${CRATE_NAME}/Cargo.toml."*
    sed '/resolver = "1"/d' "crates/${CRATE_NAME}/Cargo.toml" > "crates/${CRATE_NAME}/Cargo.toml.tmp"
    mv "crates/${CRATE_NAME}/Cargo.toml.tmp" "crates/${CRATE_NAME}/Cargo.toml"
}

# Get the latest version from artifact links at: https://code.amazon.com/packages/AWSVectorConsolasRuntimeServiceRustClient/releases#
# for example, https://prod.artifactbrowser.brazil.aws.dev/packages/AWSVectorConsolasRuntimeServiceRustClient/versions/0.1.8200.0/platforms/AL2_x86_64/flavors/DEV.STD.PTHREAD
# Note that a `.0` is appended as part of the above script.
CONSOLAS_VERSION="0.1.13922"

update_pkg "AWSVectorConsolasRuntimeServiceRustClient" "$CONSOLAS_VERSION" "amzn-codewhisperer-client"
update_pkg "AWSVectorConsolasRuntimeServiceRustClient" "$CONSOLAS_VERSION" "amzn-codewhisperer-streaming-client"
update_pkg "AWSVectorConsolasRuntimeServiceRustClient" "$CONSOLAS_VERSION" "amzn-consolas-client"
# This was removed
# update_pkg "AWSVectorConsolasRuntimeServiceRustClient" "$CONSOLAS_VERSION" "amzn-qdeveloper-client"
update_pkg "AWSVectorConsolasRuntimeServiceRustClient" "$CONSOLAS_VERSION" "amzn-qdeveloper-streaming-client"

# update_pkg "FigIoToolkitTelemetryLambdaClientRust" "1.0.0" "amzn-toolkit-telemetry"

cargo clippy --fix --allow-dirty

# this has to run twice for some reason, thanks rust-fmt
cargo +nightly fmt
cargo +nightly fmt
