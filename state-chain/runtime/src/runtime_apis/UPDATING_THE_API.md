
# READ THIS BEFORE UPDATING THE API TRAITS

Versioning of runtime apis is explained in the [sp-api docs](https://docs.rs/sp-api/latest/sp_api/macro.decl_runtime_apis.html).

Of course it doesn't explain everything, e.g. there's a very useful
`#[renamed($OLD_NAME, $VERSION)]` attribute which will handle renaming
of apis automatically.

If in doubt, look at the implementation of the `decl_api` macro.

## Module hierarchy

Separate API 'namespaces' are grouped into modules. Each module is structured like this:

```code
|- runtime_apis
   |- impl_api     // The impl_api block for all runtime APIs
   |- types        // define / re-export api-level types
   |- *_api        // api interface declaration
      |- types.    // API-local type definitions
```

## When changing an existing method

- Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
- Annotate the old method with #[changed_in($VERSION)] where $VERSION is the *new* api_version,
   for example #[changed_in(3)].
- Handle the old method in the custom rpc implementation using runtime_api().api_version().

## When adding a new method

- Bump the api_version of the trait, for example from #[api_version(2)] to #[api_version(3)].
- Create a dummy method with the same name, but no args and no return value.
- Annotate the dummy method with #[changed_in($VERSION)] where $VERSION is the *new*
   api_version.
- Handle the dummy method gracefully in the custom rpc implementation using
   runtime_api().api_version().
