CARGO_TARGET_DIR ?= $(CURDIR)/target
RUST_TARGETS ?= aarch64-apple-darwin x86_64-apple-darwin
ARCHIVE = QuickLookStep/libfoxtrot_universal.a
RUST_SLICES = $(foreach target,$(RUST_TARGETS),"$(CARGO_TARGET_DIR)/$(target)/release/libfoxtrot_ffi.a")

.PHONY: foxtrot.h
foxtrot.h:
	cbindgen ffi -o QuickLookStep/foxtrot.h -l c

.PHONY: libfoxtrot_universal.a
libfoxtrot_universal.a:
	@set -e; for target in $(RUST_TARGETS); do \
		cargo build --locked --release --target-dir "$(CARGO_TARGET_DIR)" --target "$$target" -p foxtrot_ffi; \
	done
	lipo -create $(RUST_SLICES) -output "$(ARCHIVE).tmp"
	mv "$(ARCHIVE).tmp" "$(ARCHIVE)"

.PHONY: test-foxtrot
test-foxtrot:
	cargo test --locked --target-dir "$(CARGO_TARGET_DIR)" -p foxtrot_ffi

.PHONY: xcodebuild
xcodebuild: libfoxtrot_universal.a
	xcodebuild \
	  -project QuickLookStep/QuickLookStep.xcodeproj \
	  -scheme QuickLookStep \
	  -configuration Debug \
	  -destination 'generic/platform=macOS' \
	  -derivedDataPath build \
	  CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM= \
	  build
