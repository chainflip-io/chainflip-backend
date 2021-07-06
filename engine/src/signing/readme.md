# Signing Module

The signing module facilitates the creation of a key that is shared by the chosen validators and then using a created key to sign stuff.

## Communicate with the other modules

### external communication

`MultisigClient` uses a channel with the `Subject::MultisigEvent` as the subject on all outgoing messages to the stream.
It subscribes to `P2PIncoming`, `MultisigInstruction`.
It publishes to `MultisigEvent` with a `ReadyToKeygen`,`ReadyToSign`,`MessageSigned` or `KeygenResult` message.

### Internal communication

`MultisigEvent` has a message that is an `InnerEvent`.
The `InnerEvent` is an enum with the relevant data to go to the `MultisigClientInner` and below.

### Structure diagram

```mermaid
graph TD;
MultisigClient-->MultisigClientInner;
MultisigClientInner-->SigningStateManager;
MultisigClientInner-->KeygenManager;
SigningStateManager-->SigningState;
SigningState-->SharedSecretState;
KeygenManager-->KeygenState;
KeygenState-->SharedSecretState
```

>viewable in markdown preview enhanced or with "GitHub + Mermaid" chrome extension.

### `MultisigClient`

Async stuff and handles the message que to and from the channel.
Takes the `InnerEvent`s and publishes the relative `MultisigEvent`.
Uses the `InnerEvent::InnerSignal` to communicate with the internal modules below.

### `MultisigClientInner`

high level keygen and signing. Gets a `MultisigInstruction` message and does one of those 2 things.
Once a procedure is complete, it will send an `InnerEvent::InnerSignal` with the `KeyReady` or `MessageSigned` enum.

### `SigningStateManager`

Routes the messages to the correct `SigningState` process, so multiple signs can happen at once. Also handles buffering/delaying requested signs if the `SigningState` is not ready to sign it yet.
Runs a cleanup when told. The cleanup checks for timeouts.
If a timeout happens, the manager shows a warning, no blame is issued.

### `SigningState`

The `SigningState` takes the message and progresses the signing procedure using the `SharedSecretState`.
It also handles buffering incoming `Secret2`s and `LocalSig`s until the `SigningStage` in in the state to use them.

### `KeygenManager`

Routes the messages to the correct `KeygenState` process, so multiple keygens can happen at once.
Runs a cleanup when told. The cleanup checks for timeouts.
If a timeout happens, the manager shows a warning, no blame is issued.

### `KeygenState`

The `KeygenState` takes the message and progresses the Keygen procedure using the `SharedSecretState`.
It also handles buffering incoming `Secret2`s until the `KeygenState` in in the state to use them.

## `SharedSecretState`

### phase 1

So the `SharedSecretState` receives the bc1 broadcast from the other validators containing the `Broadcast1`,
Once it has received enough (share_count), it will put them in order and change its `StageStatus` to `Full`, ready for the next phase.
If the `SharedSecretState` gets a duplicate idx, it shows an error with the idx and goes to the Ignored `StageStatus`. The `SigningStage`/`KeygenStage` will ignore it and move on.

### phase 2

In Phase 2 it verifies the accumulated secrets and creates a `Secret2` for each validator.
Then sends the `Secret2` to each of the corresponding validators and stores its own secret.
If the verify was unsuccessful, it returns an error and relies on the parent to abandon the keygen.(todo).
The id of the culprit is not calculated. No blame is issued. (todo).
We then wait for all the `shared_secrets` to come in from the other validators. Once full it moves to phase 3.

### phase 3

In phase 3 it will verify `shared_secrets` and construct the key pair.
If it is valid and being used by the `keygen_state`, then it will go to `KeygenStage::KeyReady`.
Once again it relies on the parent to abandon the process if invalid and no blame is issued. (todo).

### Local Sigs

If the `SharedSecretState` is being used by the `SingingState`, then `SingingState` will continue to the `AwaitingLocalSig3` after phase 3.
While in `AwaitingLocalSig3`  it collects the signatures from all of the validators and its self.
Once full, it aggregates them and verifies it using the aggregated public key generated in phase 2.
If the verification fails, it shows a warning and no blame is issued. (todo)
Should there be a failure state in `SigningStage` so it can be cleaned up? Does the SigningStateManager just wait for timeout.

### bitcoin_schnorr.rs

Contains part of the Multisig Schnorr library. Implementation for `Keys`, `LocalSig` and `Signature`, used by the `SharedSecretState`.

### TODO

- Move todo list from `client_inner/tests/mod.rs` to here?

## Tests

### Keygen tests

- bc1_gets_delayed_until_keygen_request
- keygen_message_from_invalid_validator
- keygen_secret2_gets_delayed
- can_have_multiple_keys
- cannot_create_key_for_known_id
- no_keygen_request
- phase1_timeout
- phase2_timeout
- invalid_bc1
- invalid_sec2

### Signing tests

- should_await_bc1_after_rts
- should_process_delayed_bc1_after_rts
- delayed_signing_bc1_gets_removed
- signing_secret2_gets_delayed
- signing_local_sig_gets_delayed
- request_to_sign_before_key_ready
- unknown_signer_ids_gracefully_handled
- no_sign_request
- phase1_timeout
- phase2_timeout
- phase3_timeout (local_sig_timeout)
- sign_request_from_invalid_validator
- cannot_create_sign_for_known_id

### Client tests

- distributed_signing (ignore)
