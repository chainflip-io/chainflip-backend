# Signing Readme
The signing module facilitates the creation of key that is shared by the chosen validators and then using the key to sign stuff.

### Communicate with the other modules:
`MultisigClient` uses a channel with the `MultisigEvent` as the subject on all outgoing messages to the stream.
The `MultisigClient` subscribes to `P2PIncoming`, `MultisigInstruction` and `ReadyToKeygen`.
MultisigEvent has a message that is an InnerEvent.
The InnerEvent is an enum with the relevant data.

### Structure
```
MultisigClient ->
	MultisigClientInner ->
		SigningStateManager -> SigningState -> SharedSecretState
        KeygenManager -> KeygenState -> SharedSecretState
```

### MultisigClient
Async stuff and handles the message que from the channel.

### MultisigClientInner
high level keygen and signing. Gets a `MultisigInstruction` from somewhere and does one of thoes 2 things.

### SigningStateManager
Routes the messages to the correct `SigningState` process, so multiple signs can happen at once.
The `SigningState` takes the message and progresses the signing procedure using the `SharedSecretState`.

### KeygenManager
Routes the messages to the correct `KeygenState` process, so multiple keygens can happpen at once.
The `KeygenState` takes the message and progresses the Keygen procedure using the `SharedSecretState`.

## SharedSecretState
So the SharedSecretState recieves the bc1 broardcast from the other validators containing the blind and the Point,
Once it has received enough (share_count), it will put them in order and change its StageStatus to Full, ready for the next phase.
If the SharedSecretState gets a duplicate idx, it shows an error with the idx and goes to the Ignored StageStatus. The SigningStage/KeygenStage will ignore it and move on.
..Why would this happen?

In Phase 2 it verifies the accumulated secrets and creates a secret_shares for each validator.
Then sends the secret_shares to each of the corresponding validators and stores out own secret.
If the verify was unsuccessful, it returns an error and relies on the parent to abandon the  (todo)keygen.
The id of the culprit is not calculated. No blame is issued. (todo).
We then wait for all the shared_secrets to come in from the other validators. Once full it moves to phase 3.

In phase 3 it will verify shared_secrets and construct the key pair.
Once again it relies on the parent to abandon the process if invalid and no blame is issued. (todo)

If the SharedSecretState is being used by the SingingState, then SingingState will continue to the AwaitingLocalSig3 after phase 3.
While in AwaitingLocalSig3  it collects the signatures from all of the validators and its self. 
Once full, it aggregates them and verifies it using the aggregated public key generated in phase 2.
If the verification fails, it shows a warning and no blame is issued. (todo)
Should there be a failure state in SigningStage so it can be cleaned up? Does the SigningStateManager just wait for timeout.

The timeout is implemented at the manager level, not in the SharedSecretState.
The managers runs a cleanup when told.
todo: make then run cleanup periodically.
If a timeout happens, the manager shows a warning, no blame is issued.. TODO: send a signal. 
TODO: successful ceremonies should clean up themselves!

The KeygenOutcome contains a list of validators to blame, but the list is never filled in at the failure points.