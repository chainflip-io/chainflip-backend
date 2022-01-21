# Substrate Troubleshooting

This is a living document with tips, gotchas and general subtrate-related wizardry. If you ever get stuck with an incomprehensible compiler error, before spending the day turning in circles, come here first and see if someone else has already encountered the same issue. 

Please add anything you think will save your colleagues' precious time. 

If you come across anything that is inaccurate or incomplete, please edit the document accordingly. 

As of yet there is no real structure - this isn't intended to be a document to read from start to finish, it's not a tutorial. But your entries should be searchable, so write your entries with SEO in mind. 


## Runtime upgrades / Try-runtime

### General tips

- There are some useful storage conversion utilities in `frame_support::storage::migration`.
- Don't forget the add the `#[pallet::storage_version(..)]` decorator.

### Storage migration / OnRuntimeUpgrade hook doesn't execute

Make sure you build with `--features try-runtime`.
Make sure you have incremented the spec version and/or the transaction version in `runtime/lib.rs`.
Make sure you are testing against a network that is at a lower version number!

### Pre and Post upgrade hooks don't execute

Make sure to add `my-pallet/try-runtime` in the runtime's Cargo.toml, otherwise the feature will not be activated for the pallet when the runtime is compiled. 

## Benchmarks

...
