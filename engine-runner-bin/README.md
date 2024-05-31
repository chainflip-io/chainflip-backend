# Chainflip Engine (Runner)

This README describes how the Chainflip Engine (CFE) actually runs and how the components interact to come together into a single runnable binary.

The Chainflip Engine as it is run by a validator, consists of 3 build artefacts:

1. The `engine-runner` binary.
2. A shared library of the old [engine-dylib](./../engine-dylib/). This is fetched from the previous release build when a release is built. Or directly from the repo ([`old-engine-dylib](./../old-engine-dylib/)) for localnets
3. A shared library of the new [engine-dylib](./../engine-dylib/). This is built during the `cargo build` of the current version.

> The new version is always at least a minor version greater than the old. We use semver minor (or major) version to imply a breaking API change. In this case, we would require the new version to take the *last* release as the old version.
e.g. If we're at version 1.3.0 and we require a breaking change. This would be called 1.4.0 and the 3 artefacts included in the release would be: `engine-runner`, `libchainflip_engine_v1_3_0` and `libchainflip_engine_v1_4_0`.
`cargo-deb` handles the packaging, see the [toml file](./Cargo.toml) for more information.

The runner binary is responsible for loading the shared libraries and running each of the shared libraries. The basic flow is as follows, using old version 1.3.0 and new version 1.4.0 as an example:

1. The runner binary is started.
2. The runner binary runs 1.4.0 shared library. It either:
    i. Runs as normal. If the runtime has been updated, and the compatible CFE version is 1.4.0, then this shared library will run.
    ii. Return out with an ExitStatus signalling that this version is *not yet* compatible with the runtime. If this occurs we move to step 3.
    iii. Return out with an ExitStatus signalling that this version is *no longer* compatible with the runtime. Implying that the runner binary
    is too old and the operator needs to upgrade.
3. 1.4.0 is not compatible, and so we run the 1.3.0 shared library. This will run as normal until...
4. Runtime upgrade occurs. This changes the compatible CFE version to be 1.4.0. Now the 1.3.0 binary will return out with an ExitStatus signalling that this version is *no longer* compatible with the runtime.
5. The runner binary will then run the 1.4.0 shared library again. This time it will run as normal.

## Preparing an Upgrade

1. Bump the `NEW_VERSION` and `OLD_VERSION` consts in the [engine-upgrade-utils](./../engine-upgrade-utils/src/lib.rs) to the new version.
All the versions that are required to be bumped are checked in the `build.rs` files of each crate *at compile time*. This is an important property that ensures mistakes cannot be made when upgrading the versions.
2. Bump the versions as specified by the build.rs scripts. This includes the package versions, the dylib name, and the assets in the [toml file](./Cargo.toml).

### Developer Notes

- The `cfe_entrypoint`, which is the C FFI entrypoint of the dylib that the runner can call, has its version defined by the version specified in the `engine-proc-macros` crate, and NOT the version of the dylib crate itself. This is perhaps a little counterintuitive, but it's just how Rust environment variables and proc-macros work at the moment. This is why the proc-macros crate checks it's version against the `NEW_VERSION` and `OLD_VERSION` consts in it's [build.rs](./../engine-proc-macros/build.rs) file.
- So `DYLD_LIBRARY_PATH` does not need setting, when building the dylib locally for mac, you will have to run `install_name_tool` to remove the localpath and use `@rpath` so it can be used dynamically. After building the dylib you can run:

 ```shell
 install_name_tool -id @rpath/libchainflip_engine_v1_4_0.dylib ./target/release/libchainflip_engine_v1_4_0.dylib
 ```

The `LC_RPATH` is already set in the runner in the `build.rs` file.
