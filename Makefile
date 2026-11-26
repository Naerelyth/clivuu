CARGO_TARGET_DIR ?= /tmp/clivuu-target
export CARGO_TARGET_DIR

PACKAGE_BUILD_DIR ?= /tmp/clivuu-package
PACKAGE_OUTPUT ?= /tmp/clivuu-dist
APPIMAGETOOL ?= appimagetool
APPIMAGE_RUNTIME ?=

.PHONY: build test check package-configure package deb rpm appimage
build:
	cargo build --release --locked -p clivuu -p clivuu-desktop --bins

test:
	cargo build --locked
	cargo test --workspace --locked
	node tests/i18n.mjs
	GIO_USE_VFS=local dbus-run-session -- gjs -m tests/cli-readiness.js $(CARGO_TARGET_DIR)/debug/clivuu
	python3 -B tests/config.py
	python3 -B tests/package_native.py
	python3 -B tests/documentation.py
	python3 -B tests/toolbar_parity_test.py

check:
	cargo fmt --all --check
	cargo clippy --workspace --locked --all-targets -- -D warnings

package-configure: build
	cmake -S . -B "$(PACKAGE_BUILD_DIR)" \
		-DCLIVUU_PREBUILT_DIR="$(CARGO_TARGET_DIR)/release" \
		-DCLIVUU_PACKAGE_OUTPUT="$(PACKAGE_OUTPUT)" \
		-DCLIVUU_PACKAGE_CONTACT="$(PACKAGER)" \
		-DCLIVUU_APPIMAGETOOL="$(APPIMAGETOOL)" \
		-DCLIVUU_APPIMAGE_RUNTIME="$(APPIMAGE_RUNTIME)"

package: package-configure
	cpack --config "$(PACKAGE_BUILD_DIR)/CPackConfig.cmake" -G TGZ

deb: package-configure
	cpack --config "$(PACKAGE_BUILD_DIR)/CPackConfig.cmake" -G DEB

rpm: package-configure
	cpack --config "$(PACKAGE_BUILD_DIR)/CPackConfig.cmake" -G RPM

appimage: package-configure
	cpack --config "$(PACKAGE_BUILD_DIR)/CPackConfig.cmake" -G External
