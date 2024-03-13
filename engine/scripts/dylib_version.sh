#!/bin/bash
#
# Changes the name of dylib (or .so file if on Linux) to include the version of the engine shared library.
# This is necessary because Rust does not allow you to specify a crate/static library name that's different to the dylib name.
# Thus, to avoid changing the name of the statically linked dependencies of the engine (e.g. in the CLI) we just rename the dylib build with this script
# after the build, using cargo-make.

echo "target_dir: $CARGO_MAKE_CRATE_TARGET_DIRECTORY"

target_dir=$CARGO_MAKE_CRATE_TARGET_DIRECTORY

# Determine the profile subdirectory
if [ "$CARGO_MAKE_PROFILE" == "development" ]; then
  profile_sub_dir="debug"
else
  profile_sub_dir="release"
fi

build_dir="$target_dir/$profile_sub_dir"

# Read and parse Cargo.toml to get version
version=$(grep -m 1 '^version = ' "$CARGO_MAKE_WORKING_DIRECTORY/Cargo.toml" | sed -E 's/version = "([0-9]+)\.([0-9]+)\.([0-9]+)"/\1_\2_\3/' | tr -d '\n')

echo "the version is: $version"

if [ "$CARGO_MAKE_RUST_TARGET_OS" == "macos" ]; then
  extension="dylib"
else
  extension="so"
fi

echo "extension: $extension"

# Determine file names for renaming
# Note: Adjust the extension as necessary for your OS. This example uses .dylib for macOS.
original="$build_dir/libchainflip_engine.${extension}"
renamed_with_version="$build_dir/libchainflip_engine_v${version}.${extension}"

echo "Renaming $original to $renamed_with_version"

# Rename the file
if ! mv "$original" "$renamed_with_version"; then
  echo "Failed to rename the artifact" >&2
fi