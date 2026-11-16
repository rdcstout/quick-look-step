# Development

The app, Finder preview extension, and thumbnail extension are built together. The app opens local STEP files. Finder routes previews and thumbnails by the file's resolved type identifier.

## Build and test

Use Xcode on an Apple Silicon Mac with the Rust `rustup` toolchain manager on `PATH`. The repository pins Rust and its macOS targets in `rust-toolchain.toml` and uses `Cargo.lock` for dependency resolution.

From the repository root:

```sh
make test-foxtrot xcodebuild
```

This runs the Rust bridge's fixture tests, builds the archive from the vendored Foxtrot source, and builds an ad-hoc-signed Debug app at `build/Build/Products/Debug/QuickLookStep.app`. Run `make libfoxtrot_universal.a` before using the Xcode project directly after a fresh checkout or Rust change.

## Finder verification

Open the built app once. In System Settings under General, open Login Items & Extensions and enable the Quick Look extensions for the development build. Select a copy of `foxtrot/examples/cube_hole.step` or `foxtrot/examples/abstract_pca.step` in Finder and press Space. Confirm the model appears and can be rotated. Close the preview and independently confirm the icon or column-view thumbnail. Repeat using the `.stp` extension and files exported by the CAD apps affected by the change.

When Finder shows only a generic file icon, check the resolved type with `mdls -name kMDItemContentType <file>` and the preview registration with `pluginkit -m -vv -i com.rdcstout.QuickLookStep.StepPreview`. Check `com.rdcstout.QuickLookStep.StepThumbnail` for thumbnails.

## Continuous integration and signing

The [GitHub Actions workflow](.github/workflows/build.yml) runs the build and Rust checks on macOS 26 and the Xcode 27 runner. CI does not exercise the Finder interface. Developer ID signing and notarization run through the manual [candidate packaging workflow](.github/workflows/README.md). Install and verify the exact package before publishing it.
