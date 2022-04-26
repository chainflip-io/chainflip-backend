# custom-rpc

In this package, we define custom RPCs for our runtime API.

## What is a custom RPC in Substrate?

A custom RPC gives us the possibility to extend our node with extra endpoints. In combination with a runtime API, we can build interfaces to access more complex information or perform complex business logic via an easy-to-use interface.

## Integrate an RPC

### Runtime API

To define a runtime API you simply have to add:

1. A new trait function to our `CustomRuntimeApi` in the `state-chain/runtime/src/runtime_apis.rs` file.
2. The implementation of this trait function in the `state-chain/runtime/src/lib.rs` inside the `impl_runtime_apis!` macro.

### RPC

To extend the custom RPC you simply have to:

1. Add a new function to the `CustomApi` trait for your RPC endpoint.
2. Add the implementation of that trait function inside the `impl` block for the `CustomRpc`struct.

> **_NOTE:_**  The implementation of the RPC part should be quite similar to the existing ones. It should be straightforward to copy/paste and modify the existing examples.
