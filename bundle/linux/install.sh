#!/bin/sh

# Installs the q and qterm into place on the user's machine
# and installs the recommended integrations

set -o errexit
set -o nounset

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"

log_error() {
    printf '\e[31m[ERROR]\e[0m %s\n' "$1" >&2
}

target_triple() {
    BUILD_INFO_PATH="$SCRIPT_DIR/BUILD-INFO"
    if [ ! -f "$BUILD_INFO_PATH" ]; then
        log_error "BUILD-INFO file not found."
        return 1
    fi

    target_triple_line=$(grep '^BUILD_TARGET_TRIPLE=' "$BUILD_INFO_PATH")
    if [ -z "$target_triple_line" ]; then
        log_error "BUILD_TARGET_TRIPLE not found in BUILD-INFO."
        return 1
    else
        echo "${target_triple_line#BUILD_TARGET_TRIPLE=}"
    fi
}

target_triple_uname() {
    target_triple=$(target_triple)
    case "$target_triple" in
        *linux*)
            echo "Linux"
            ;;
        *darwin*)
            echo "Darwin"
            ;;
        *windows*)
            echo "Windows"
            ;;
        *)
            log_error "Could not determine OS."
            return 1
            ;;
    esac
}

is_target_triple_gnu() {
    target_triple=$(target_triple)
    if [ "${target_triple##*-}" = "gnu" ]; then
        return 0
    else
        return 1
    fi
}

# Minimum required glibc version
GLIBC_MIN_MAJOR=2
GLIBC_MIN_MINOR=34

# Check if a glibc version meets the minimum requirement
is_glibc_version_sufficient() {
    local version="$1"
    local major minor

    IFS='.' read -r major minor <<EOF
$version
EOF
    if [ -z "$minor" ]; then
        minor=0
    fi

    if [ "$major" -gt "$GLIBC_MIN_MAJOR" ] || { [ "$major" -eq "$GLIBC_MIN_MAJOR" ] && [ "$minor" -ge "$GLIBC_MIN_MINOR" ]; }; then
        return 0
    else
        return 1
    fi
}

# checks that the system has at least glibc 2.34
check_glibc_version() {
    # Method 1: Original approach - try common libc.so.6 locations
    for LIBC_PATH in /lib64/libc.so.6 /lib/libc.so.6 /usr/lib/x86_64-linux-gnu/libc.so.6 \
        /lib/aarch64-linux-gnu/libc.so.6; do
        if [ -f "$LIBC_PATH" ]; then
            glibc_version=$("$LIBC_PATH" | sed -n 's/^GNU C Library (.*) stable release version \([0-9]*\)\.\([0-9]*\).*$/\1.\2/p')
            if [ -n "$glibc_version" ]; then
                if is_glibc_version_sufficient "$glibc_version"; then
                    return 0
                else
                    return 1
                fi
            fi
        fi
    done

    # Method 2: Try ldd --version as a more reliable alternative
    if command -v ldd >/dev/null 2>&1; then
        glibc_version=$(ldd --version 2>/dev/null | head -n 1 | grep -o '[0-9]\+\.[0-9]\+' | head -n 1)
        if [ -n "$glibc_version" ]; then
            if is_glibc_version_sufficient "$glibc_version"; then
                return 0
            else
                return 1
            fi
        fi
    fi

    # Method 3: Try getconf as a fallback
    if command -v getconf >/dev/null 2>&1; then
        glibc_version=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')
        if [ -n "$glibc_version" ]; then
            if is_glibc_version_sufficient "$glibc_version"; then
                return 0
            else
                return 1
            fi
        fi
    fi

    log_error "Could not determine glibc version. This CLI requires glibc $GLIBC_MIN_MAJOR.$GLIBC_MIN_MINOR or newer."
    return 1
}

# checks that uname matches the target triple
if [ "$(uname)" != "$(target_triple_uname)" ]; then
    log_error "This archive is built for a $(target_triple_uname) system."
    exit 1
fi

if is_target_triple_gnu && ! check_glibc_version; then
    log_error "This release built for a GNU system with glibc $GLIBC_MIN_MAJOR.$GLIBC_MIN_MINOR or newer, try installing the musl version of the CLI."
    exit 1
fi

if [ -n "${Q_INSTALL_GLOBAL:-}" ]; then
    install -m 755 "$SCRIPT_DIR/bin/q" /usr/local/bin/
    install -m 755 "$SCRIPT_DIR/bin/qterm" /usr/local/bin/

    /usr/local/bin/q integrations install dotfiles
    /usr/local/bin/q setup --global "$@"
else
    mkdir -p "$HOME/.local/bin"

    install -m 755 "$SCRIPT_DIR/bin/q" "$HOME/.local/bin/"
    install -m 755 "$SCRIPT_DIR/bin/qterm" "$HOME/.local/bin/"

    "$HOME/.local/bin/q" setup "$@"
fi
