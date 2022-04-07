# custom-rpcs

In this package, we define custom RPCs for our runtime APIs.

## Integrate an RPC

### Naming schema

If you define new RPC APIs please follow this naming schema:

- RuntimeApi -> {Your}RuntimeApi
- RpcApi -> {Your}Api
- Struct we implement the API on -> {Your}Rpc

### Runtime API

To expose logic from the runtime to our node we first have to define a runtime API. We've to do basically two things:

1) Define a trait for your runtime API in the `./state-chain/runtime/runtime_apis.rs` (if not already done)
2) Implement your trait in the `./state-chain/runtime/lib.rs` inside the `impl_runtime_apis!` macro.

### RPC

1) Copy the `meaning_of_life_rpc.rs` file to use it for your RPC.
2) Renaming everything based on the naming schema.
3) Add the new file in the `lib.rs` file.

### Install the RPC

Open the `./state-chain/node/src/services.rs` file and install the rpc in the `new_full` function inside `rpc_extensions_builder`:
```rust
io.extend_with(YourApi::to_delegate(YourRpc {
    client: client.clone(),
    _phantom: PhantomData::default(),
}));
```
