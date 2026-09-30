# WebAuthn signing PoC (PRO-3134)

Spike exploring option C of *Research: widening hardware signing support*: governance members
signing with a FIDO security key (YubiKey) through WebAuthn, with hardware enforced by verifying
the key's attestation on chain at registration.

## What's here

| Component | Path | Purpose |
|---|---|---|
| `cf-webauthn` | `state-chain/webauthn` | `no_std` ES256 assertion verification, `packed` attestation verification, `CfSignature` (path 1) |
| `pallet-cf-webauthn` | `state-chain/pallets/cf-webauthn` | Registry of attested credentials, `register_credential`, `CheckWebAuthn` extension (path 2) |
| Recording page | `state-chain/webauthn/poc-page` | `bun run server.ts`, then Chrome at http://localhost:8765 |
| Fixtures | `state-chain/webauthn/fixtures` | Real YubiKey recordings, Yubico FIDO issuing CAs (DER) |
| Runtime tests | `state-chain/cf-integration-tests/src/webauthn.rs` | Both paths approving a governance proposal |

Two signing paths share the verifier and registry:

- **Path 1: `MultiSignature` variant.** The runtime `Signature` becomes `CfSignature`
  (`Ed25519 | Sr25519 | Ecdsa | WebAuthn`). Transactions are ordinary v4 signed extrinsics. The
  WebAuthn challenge is `blake2_256` of what `Verify::verify` receives.
- **Path 2: transaction extension.** `CheckWebAuthn` is first in `TxExtension` and authorises v5
  general transactions. The challenge is `blake2_256(ext_version ‖ call ‖ following extensions'
  explicit ‖ implicit)`.

Both derive the account as `blake2_256("chainflip/webauthn-p256" ‖ compressed P-256 key)`, so one
credential works on either path.

## Proven

- A real YubiKey 5 NFC (AAGUID `2fc0579f-8113-47ea-b116-bb5a8db9202a`) registers on chain. The
  `packed` attestation verifies, including the RSA chain check against the pinned FIDO issuing
  CAs. Duplicates, unpinned CAs and registrations replayed by another sponsor are rejected.
- Signing through each path, that YubiKey approves a governance proposal as a member with no
  native key, in the full runtime.
- Path 2 cannot be replayed (`CheckNonce` applies to general transactions), rejects unregistered
  credentials before any signature check, and charges fees to the authorised account.
- The full `cf-integration-tests` and `state-chain-runtime` test suites pass with both paths
  wired in.

## Findings

### Common to both paths

- **Registration needs a sponsor.** The new account has no native key and doesn't exist until
  someone creates it. `register_credential` is submitted by a sponsor and bumps `sufficients`;
  without that, `CheckNonce` rejects every transaction from the account.
- **Registration can't bind the new account.** The challenge binds genesis hash and sponsor. The
  credential key doesn't exist until `create()` returns, so nothing ties it to the intended
  person. A follow-up transaction signed by the new key (as in PRO-3153's self-approval) is the
  natural binding.
- **The credential id must be stored on chain** (64 bytes here). Security-key credentials aren't
  discoverable, so clients need the id for `allowCredentials`.
- **Attestation shape.** `packed`/ES256; `x5c` held only the leaf (`Yubico U2F EE Serial …`),
  issued directly by `Yubico U2F Root CA Serial 457200631`. All five Yubico FIDO issuing CAs are
  RSA-2048, so the runtime needs `rsa` regardless.
- **No user verification by default.** Both recorded signatures have flags `0x01`: user present,
  not verified. A PIN requirement needs `userVerification: "required"` in the page,
  `require_user_verification` in the policy, and a PIN set on every key.
- **Size.**
  - Attestation object: 1,051 B.
  - Assertion on chain: about 238 B (authenticator data 37, `clientDataJSON` 134, signature 64).
  - `clientDataJSON` embeds the page origin, so a long domain eats into its 512 B bound.
- **Runtime WASM:** compressed 5,136,268 → 5,255,386 B (+2.3%) for both paths plus attestation.
  New dependencies: `p256`, `rsa`, `x509-cert`, `ciborium`. `rsa` 0.9 carries RUSTSEC-2023-0071
  (timing, private-key operations only; we only verify), so `cargo audit` will need an ignore.
- **Verification cost.** P-256 has no host function, so the runtime runs it as WASM. Benchmarks
  (`pallet-cf-webauthn/src/benchmarking.rs`, compiled WASM, Apple Silicon laptop, not the
  reference machine):

  | Benchmark | Time | vs sr25519 |
  |---|---|---|
  | sr25519 verify (host function), baseline | 24 µs | 1× |
  | Path 1: `CfSignature::WebAuthn` verify | 607 µs | 25× |
  | Path 2: `CheckWebAuthn` validate (+2 reads) | 613 µs | 26× |
  | `register_credential` (attestation, RSA chain, 5 anchors) | 1,029 µs | — |

  For scale, `ExtrinsicBaseWeight` (the whole per-transaction base, reference hardware) is
  108 µs. Native verification is about 167 µs, so WASM costs roughly 3.6× native.
- **Registration also writes `AccountRoles`.** Creating the account runs the runtime's
  new-account hook, so a WebAuthn account gets an account-role entry that cleanup must also
  cover.
- **Recorded fixtures go stale on purpose.** Signed payloads commit to the genesis hash and spec
  version. The YubiKey runtime tests are `#[ignore]`d; re-record as described below.

### Path 1 (`MultiSignature` variant)

- **Small change.** Only the runtime `Signature` type changes. Variants 0–2 encode exactly like
  `MultiSignature`. `CfSigner` needs `From<{ed25519,sr25519,ecdsa}::Public>` for existing signers.
  `cf-primitives` can keep `MultiSignature` (`AccountId32` is unchanged).
- **No storage access in `Verify`.**
  - No registry check: any P-256 WebAuthn key, synced passkeys included, can sign once its
    account exists.
  - No per-signature policy: RP id, UV and backup-eligible checks can't be enforced.
  - Hardware enforcement has to happen a level up, e.g. governance only admits accounts present
    in the attested registry.
- **Mispriced.** Signature checks are covered by `ExtrinsicBaseWeight`, which is calibrated for
  host-function sr25519. Every valid WebAuthn transaction under-pays by about 0.6 ms (more than
  5× the entire base weight), and each garbage submission costs a validator a full 0.6 ms check
  for free, against 24 µs for sr25519. `Verify` can't read storage, so there's no cheap
  pre-check and no way to add the missing weight.
- **Decoding.** Indexers and explorers that hardcode `MultiSignature` can't decode blocks
  containing the new variant.

### Path 2 (`CheckWebAuthn` extension)

- **What it gets right.** It charges explicit weight, looks up the registry before verifying,
  applies per-signature policy, and leaves `MultiSignature` alone.
- **On `stable2509` it breaks every transaction, not just WebAuthn ones.** `CheckWebAuthn`'s
  `Option` adds a byte to every signed transaction and to what is signed, so it isn't backward
  compatible for regular transactions:
  - polkadot-js 16.1.1 treats unknown extensions as "no-effect" (`types/create/registry.js`) and
    silently omits the byte. Every transaction it builds (bouncer, cf-gov-js, polkadot.js apps)
    becomes undecodable until each client registers `CheckWebAuthn` as a custom extension.
  - Every Rust site that constructs `TxExtension` must add `CheckWebAuthn::disabled()` and `()`,
    and engines built for the old runtime fail to transact until upgraded.
  - `stable2509` has no versioned extension pipelines, so none of this can be avoided there.
- **`stable2609` removes that problem** (the SDK upgrade in progress). `UncheckedExtrinsic` takes
  pipelines for extension versions other than 0. v4 signed transactions always use version 0;
  v5 general transactions carry a version byte that selects the pipeline. Keeping today's tuple
  as version 0 and putting `CheckWebAuthn` only in a version-1 pipeline means:
  - Existing transactions and clients encode, sign and validate exactly as before. The 2509-era
    changes to the Rust construction sites become unnecessary.
  - `CheckWebAuthn` can be mandatory in version 1 instead of an `Option`.
  - The signed message already starts with the extension version byte, so a version-1
    signature can't be replayed under version 0.
  - Only the WebAuthn client (the governance app) builds v5 transactions with extension
    version 1.
  - Discovery needs metadata V16, which lists the extensions for each version; V14 and V15 only
    expose version 0. Tooling that builds version-1 transactions (the governance app) must read
    V16 or hardcode the layout. v4 clients keep working unchanged.
  - Still to check on 2609: that the runtime serves V16 via `metadata_at_version(16)`, and that
    indexers without v5 support fail only on blocks containing WebAuthn transactions.
- **Fail-closed until configured.** With an empty genesis RP id, registration and path 2 refuse
  everything until governance sets one. Path 1 ignores the RP id.

## Recommendation

**Take path 2 (`CheckWebAuthn`).** Hardware enforcement is achievable on either path, provided
governance membership requires an entry in the attested registry, so the decision came down to
the cost of WASM P-256 verification.

- At 607 µs on a fast laptop (likely more on the reference machine), it is 25× sr25519 and more
  than 5× the whole `ExtrinsicBaseWeight`.
- Path 1 can't charge for that or pre-filter it. Anyone who funds a P-256 account gets 0.6 ms of
  validator time per transaction at sr25519 prices, and invalid signatures cost it for free.
- Path 2 charges the benchmarked weight and rejects unregistered credentials with a storage read
  before any crypto. The residual free-verification exposure is limited to registered
  credentials.

On `stable2509` the cost of path 2 is a network-wide breaking change (see above). On
`stable2609` it isn't: with `CheckWebAuthn` in a version-1 pipeline, existing transactions and
clients are untouched. Path 2 then matches path 1 on compatibility and beats it on pricing,
pre-filtering and policy, so **build path 2 on top of the 2609 upgrade**.

| | Path 1 (signature variant) | Path 2 on 2609 (version-1 pipeline) |
|---|---|---|
| Existing transactions and clients | Unchanged | Unchanged |
| Priced correctly | No (0.6 ms charged at sr25519 rates) | Yes (benchmarked weight) |
| Cheap rejection before crypto | No | Yes (registry lookup) |
| Per-signature policy (PIN, no synced keys) | No | Yes |
| Old decoders | Fail on blocks with a WebAuthn signature | Fail on v5 transactions that use version 1 |

### Not covered (scope, not doubt)

- Weights on reference hardware: the committed `weights.rs` came from a laptop, so regenerate on
  the benchmark runner before relying on the absolute numbers. The ratios hold.
- `register_credential` was benchmarked with the recorded 1,051-byte attestation, not the
  4,096-byte bound.
- Firmware 5.7.4+ keys, which chain via `Yubico FIDO Attestation {A,B,B2} 1`: pinned but not
  exercised.
- The PIN / user-verification flow.
- Browsers other than Chrome, and phones over NFC.
- Certificate validity dates, which aren't checked.
- Cleanup of per-account storage in `OnKilledAccount`.
- Localnet or RPC submission.
- The `stable2609` version-1 pipeline: the PoC wires `CheckWebAuthn` into the version-0 tuple on
  2509. Porting it re-records the YubiKey assertions, since they commit to the signed payload.

## Re-recording fixtures

1. Print the challenges:
   - `cargo test -p pallet-cf-webauthn print_registration_challenge -- --ignored --nocapture`
   - `cargo test -p cf-integration-tests webauthn::yubikey::print_challenges -- --ignored --nocapture`
2. Update `fixtures/challenges.json` with them.
3. `bun run poc-page/server.ts`, then open http://localhost:8765 in Chrome.
4. Register once (choose *security key*, allow make/model), then sign challenges 1 and 2. Don't
   re-register: it creates a new key and invalidates the signatures.
5. Run `cargo test -p cf-integration-tests webauthn::yubikey -- --ignored`.
