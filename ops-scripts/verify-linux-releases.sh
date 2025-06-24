#!/usr/bin/env bash
# shellcheck disable=SC2002

# This script downloads all of the different Linux builds under /latest
# and verifies that the BUILD-INFO is configured correctly.

set -euo pipefail

PUBLIC_DOWNLOAD_URL='https://desktop-release.q.us-east-1.amazonaws.com'
ARCHITECTURES=('x86_64' 'aarch64')
FILE_TYPES=('tar.gz' 'tar.xz' 'tar.zst' 'zip')

error() {
    echo '[ERROR]' "$@"
}

info() {
    echo '[INFO]' "$@"
}

debug() {
    echo '[DEBUG]' "$@"
}

verify() {
    local architecture="$1"
    local file_type="$2"
    local is_musl="$3"

    local fname="q-${architecture}-linux"
    if [ "$is_musl" -eq 1 ]; then
        fname+='-musl'
    fi
    fname+=".$file_type"

    info "Downloading" "$fname"
    curl "${PUBLIC_DOWNLOAD_URL}/latest/${fname}" -o "$fname"

    info "Unpacking" "$fname"
    case "$file_type" in
        'tar.gz')
            tar -xzvf "$fname"
            ;;
        'tar.xz')
            tar -xJvf "$fname"
            ;;
        'tar.zst')
            tar -I unzstd -xvf "$fname"
            ;;
        'zip')
            unzip "$fname"
            ;;
        '*')
            error "Unknown file type configured"
            exit 1
    esac

    local expected_target_triple="${architecture}-unknown-linux-"
    if [ "$is_musl" -eq 1 ]; then
        expected_target_triple+="musl"
    else
        expected_target_triple+="gnu"
    fi
    info "Checking BUILD-INFO for expected target: $expected_target_triple"
    local actual_target_triple
    actual_target_triple=$(cat q/BUILD-INFO | grep 'BUILD_TARGET_TRIPLE' | cut -d '=' -f 2)
    if [ "$expected_target_triple" != "$actual_target_triple" ]; then
        error "$fname | expected $expected_target_triple, found $actual_target_triple"
    fi

    # Every bundle unpacks to q/, hence just remove the directory before verifying the next build.
    info "Cleaning up q directory"
    rm -rf q
}

cleanup() {
    info "Removing temporary directory:" "$tempdir"
    rm -r "$tempdir"
}

main() {
    tempdir=$(mktemp -d)
    logfile='verification.txt'
    trap cleanup EXIT

    cd "$tempdir"
    info "Working directory:" "$(pwd)"
    for architecture in "${ARCHITECTURES[@]}"; do
        for file_type in "${FILE_TYPES[@]}"; do
            for is_musl in {0..1}; do
                info "Verifying architecture: $architecture, file_type: $file_type, is_musl: $is_musl"
                verify "$architecture" "$file_type" "$is_musl" >> "$logfile" 2>&1
                info "Done"
            done
        done
    done

    # Print the logfile to stderr.
    cat "$logfile" 1>&2

    # If any errors were reported during verification, then fail
    # and print them out.
    if cat "$logfile" | grep -q '^\[ERROR\]'; then
        error 'Verification failed!'
        grep '^\[ERROR\]' "$logfile"
        exit 1
    else
        info 'Verification succeeded!'
    fi
}

main

