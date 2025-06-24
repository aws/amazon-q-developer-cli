PLATFORM := $(shell uname -s)
ARCH ?= x86_64

# Check if variable contains string https://stackoverflow.com/a/2741747
ifneq (,$(findstring CYGWIN,$(PLATFORM)))
	PLATFORM := windows
endif
ifneq (,$(findstring MINGW,$(PLATFORM)))
	PLATFORM := windows
endif
ifneq (,$(findstring MSYS,$(PLATFORM)))
	PLATFORM := windows
endif

$(warning PLATFORM is $(PLATFORM))

ifeq "$(PLATFORM)" "Darwin"
# Add brew executables to PATH
export PATH := /opt/homebrew/bin:/opt/homebrew/sbin:$(PATH)
endif

# Add cargo executables to PATH
export PATH := $(HOME)/.cargo/bin:$(PATH)

ifneq "$(PLATFORM)" "windows"
SHELL := env "PATH=$(PATH)" bash
endif

ifeq "$(PLATFORM)" "windows"
MAKE_DIR    ?= $(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))) | tr '\\' '/' | sed 's/^C:/\/c/g')
TARGET_DIR  ?= $(shell cargo metadata --format-version 1 | jq -r .target_directory | tr '\\' '/' | sed 's/^C:/\/c/g')
else
MAKE_DIR    ?= $(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
TARGET_DIR  ?= $(shell cargo metadata --format-version 1 | jq -r .target_directory)
endif

BUILT_PRODUCTS_DIR ?= $(MAKE_DIR)/build
OUT_DIR            := $(BUILT_PRODUCTS_DIR)
$(shell mkdir -p $(OUT_DIR))

ifneq "$(CONFIGURATION)" "Debug"
CARGO_OPTIONS ?= --locked --release
CARGO_BUILD := release
else
CARGO_BUILD := debug
endif

BINARY_NAME ?= qterm
CRATE_NAME  ?= figterm

### Build for each platform

ifeq "$(PLATFORM)" "Darwin"
all: macos
endif
ifeq "$(PLATFORM)" "Linux"
all: linux
endif
ifeq "$(PLATFORM)" "windows"
all: windows-msvc
endif

macos: $(BINARY_NAME)-darwin-universal
	install $(OUT_DIR)/$< $(OUT_DIR)/$(BINARY_NAME)

$(BINARY_NAME)-x86_64-apple-darwin:
	cargo build --target=x86_64-apple-darwin $(CARGO_OPTIONS)
	install $(TARGET_DIR)/x86_64-apple-darwin/$(CARGO_BUILD)/$(CRATE_NAME) $(OUT_DIR)/$@

$(BINARY_NAME)-aarch64-apple-darwin:
	cargo build --target=aarch64-apple-darwin $(CARGO_OPTIONS)
	install $(TARGET_DIR)/aarch64-apple-darwin/$(CARGO_BUILD)/$(CRATE_NAME) $(OUT_DIR)/$@

$(BINARY_NAME)-darwin-universal:
	cargo build --target=x86_64-apple-darwin --target=aarch64-apple-darwin $(CARGO_OPTIONS)
	install $(TARGET_DIR)/x86_64-apple-darwin/release/$(CRATE_NAME) $(OUT_DIR)/$(BINARY_NAME)-x86_64-apple-darwin
	install $(TARGET_DIR)/aarch64-apple-darwin/release/$(CRATE_NAME) $(OUT_DIR)/$(BINARY_NAME)-aarch64-apple-darwin
	lipo -create -output $(OUT_DIR)/$@ $(OUT_DIR)/$(BINARY_NAME)-x86_64-apple-darwin $(OUT_DIR)/$(BINARY_NAME)-aarch64-apple-darwin

linux: $(BINARY_NAME)-$(ARCH)-unknown-linux-gnu
	install $(OUT_DIR)/$< $(OUT_DIR)/$(BINARY_NAME)

$(BINARY_NAME)-x86_64-unknown-linux-gnu:
	cargo build --target=x86_64-unknown-linux-gnu $(CARGO_OPTIONS)
	install $(TARGET_DIR)/x86_64-unknown-linux-gnu/$(CARGO_BUILD)/$(CRATE_NAME) $(OUT_DIR)/$@

$(BINARY_NAME)-aarch64-unknown-linux-gnu:
	cargo build --target=aarch64-unknown-linux-gnu $(CARGO_OPTIONS)
	install $(TARGET_DIR)/aarch64-unknown-linux-gnu/$(CARGO_BUILD)/$(CRATE_NAME) $(OUT_DIR)/$@

windows-gnu: $(BINARY_NAME)-x86_64-pc-windows-gnu
	install $(OUT_DIR)/$< $(OUT_DIR)/$(BINARY_NAME)

$(BINARY_NAME)-x86_64-pc-windows-gnu:
	cargo build --target x86_64-pc-windows-gnu $(CARGO_OPTIONS)
	install $(TARGET_DIR)/x86_64-pc-windows-gnu/$(CARGO_BUILD)/$(CRATE_NAME).exe $(OUT_DIR)/$@

windows-msvc: $(BINARY_NAME)-x86_64-pc-windows-msvc
	install $(OUT_DIR)/$< $(OUT_DIR)/$(BINARY_NAME)

$(BINARY_NAME)-x86_64-pc-windows-msvc:
	cargo build --target x86_64-pc-windows-msvc $(CARGO_OPTIONS)
	install $(TARGET_DIR)/x86_64-pc-windows-msvc/release/$(CRATE_NAME).exe $(OUT_DIR)/$@

### Build and Install for local dev

build-native:
	cargo build

build-native-release:
	cargo build --release

install-native: build-native
	mkdir -p $(HOME)/.local/bin/
	rm -rf $(HOME)/.local/bin/*qterm*
	install $(TARGET_DIR)/debug/$(CRATE_NAME) $(HOME)/.local/bin/$(BINARY_NAME)

install-native-release: build-native-release
	mkdir -p $(HOME)/.local/bin/
	rm -rf $(HOME)/.local/bin/*figterm*
	install $(TARGET_DIR)/release/$(CRATE_NAME) $(HOME)/.local/bin/$(BINARY_NAME)

install-native-usr: build-native
	for exe in $(shell ls /usr/bin | grep figterm | xargs -I % printf "'%'\n") figterm; do \
		sudo rm -f "/usr/bin/$${exe}"; \
		sudo install $(TARGET_DIR)/debug/$(CRATE_NAME) "/usr/bin/$${exe}"; \
	done

install-native-usr-release: build-native-release
	for exe in $(shell ls /usr/bin | grep figterm | xargs -I % printf "'%'\n") figterm; do \
		sudo rm -f "/usr/bin/$${exe}"; \
		sudo install $(TARGET_DIR)/release/$(CRATE_NAME) "/usr/bin/$${exe}"; \
	done

### Clean the project

clean:
	command -v cargo &>/dev/null && cargo clean
	rm -rf $(BUILT_PRODUCTS_DIR)

### Install for xcode
install: all

