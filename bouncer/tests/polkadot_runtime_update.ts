#!/usr/bin/env -S pnpm tsx
import Keyring from '@polkadot/keyring';
import fs from 'fs';
import path from 'path';
import assert from 'assert';
import { execSync } from 'child_process';
import { BN } from '@polkadot/util';
import { blake2AsU8a, cryptoWaitReady } from '@polkadot/util-crypto';
import { assetDecimals, Assets, Asset } from '@chainflip-io/cli';
import {
  getPolkadotApi,
  runWithTimeout,
  observeEvent,
  sleep,
  amountToFineAmount,
  Event,
} from '../shared/utils';
import {
  bumpSpecVersionAgainstNetwork,
  getCurrentRuntimeVersion,
} from '../shared/utils/bump_spec_version';
import { testSwap } from '../shared/swapping';

const PROPOSAL_AMOUNT = '100';
const POLKADOT_ENDPOINT_PORT = 9947;
const aliceUri = process.env.POLKADOT_ALICE_URI || '//Alice';
const keyring = new Keyring({ type: 'sr25519' });
const polkadot = await getPolkadotApi();

let swapsComplete = 0;
let swapsStarted = 0;
let runtimeUpgradeComplete = false;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function handleDispatchError(result: any) {
  if (result.dispatchError) {
    const dispatchError = JSON.parse(result.dispatchError);
    if (dispatchError.module) {
      const errorIndex = {
        index: new BN(dispatchError.module.index, 'hex'),
        error: new Uint8Array(Buffer.from(dispatchError.module.error.slice(2), 'hex')),
      };
      const { docs, name, section } = polkadot.registry.findMetaError(errorIndex);
      throw new Error('dispatchError:' + section + '.' + name + ': ' + docs);
    } else {
      throw new Error('dispatchError: ' + JSON.stringify(dispatchError));
    }
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function submitAndGetEvent(call: any, eventMatch: any): Promise<Event> {
  await cryptoWaitReady();
  const alice = keyring.createFromUri(aliceUri);
  let done = false;
  let event: Event = { name: '', data: [], block: 0, event_index: 0 };
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await call.signAndSend(alice, { nonce: -1 }, (result: any) => {
    if (result.dispatchError) {
      done = true;
    }
    handleDispatchError(result);
    if (result.isInBlock) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      result.events.forEach((eventRecord: any) => {
        if (eventMatch.is(eventRecord.event)) {
          event = eventRecord.event;
          done = true;
        }
      });
      if (!done) {
        done = true;
        throw new Error('Event was not found in block: ' + JSON.stringify(eventMatch));
      }
    }
  });
  while (!done) {
    await sleep(1000);
  }
  return event;
}

/// Pushes a polkadot runtime upgrade using the democracy pallet.
/// preimage -> proposal -> vote -> democracy pass -> scheduler dispatch runtime upgrade.
async function pushPolkadotRuntimeUpgrade(wasmPath: string): Promise<void> {
  console.log('-- Pushing polkadot runtime upgrade --');

  // Read the runtime wasm from file
  const runtimeWasm = fs.readFileSync(wasmPath);
  if (runtimeWasm.length > 4194304) {
    throw new Error(`runtimeWasm file too large (${runtimeWasm.length}b), must be less than 4mb`);
  }

  // Submit the preimage (if it doesn't already exist)
  const setCodeCall = polkadot.tx.system.setCode(Array.from(runtimeWasm));
  const preimage = setCodeCall.method.toHex();
  const preimageHash = '0x' + Buffer.from(blake2AsU8a(preimage)).toString('hex');
  console.log(`Preimage hash: ${preimageHash}`);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let preimageStatus = (await polkadot.query.preimage.statusFor(preimageHash)) as any;
  if (JSON.stringify(preimageStatus) !== 'null') {
    preimageStatus = JSON.parse(preimageStatus);
    if (!preimageStatus?.unrequested && !preimageStatus?.requested) {
      throw new Error('Invalid preimage status');
    }
  }
  if (preimageStatus?.unrequested?.len > 0 || preimageStatus?.requested?.len > 0) {
    console.log('Preimage already exists, skipping submission');
  } else {
    const notePreimageEvent = await submitAndGetEvent(
      polkadot.tx.preimage.notePreimage(preimage),
      polkadot.events.preimage.Noted,
    );
    assert.strictEqual(
      preimageHash,
      notePreimageEvent.data[0].toString(),
      'Preimage hash mismatch',
    );
    console.log(`Preimage submitted: ${preimageHash}`);
  }

  // Submit the proposal
  const observeDemocracyStarted = observeEvent('democracy:Started', polkadot);
  const amount = amountToFineAmount(PROPOSAL_AMOUNT, assetDecimals.DOT);
  console.log(`Submitting proposal with amount: ${PROPOSAL_AMOUNT}`);
  const democracyStartedEvent = await submitAndGetEvent(
    polkadot.tx.democracy.propose({ Legacy: preimageHash }, amount),
    polkadot.events.democracy.Proposed,
  );
  const proposalIndex = democracyStartedEvent.data[0];
  console.log(`proposal submitted: index ${proposalIndex}`);

  // Wait for the democracy started event
  console.log('Waiting for voting to start...');
  await observeDemocracyStarted; // FIXME: Sometimes this event is not getting hit?

  // Vote for the proposal
  const observeDemocracyPassed = observeEvent('democracy:Passed', polkadot);
  const observeDemocracyNotPassed = observeEvent('democracy:NotPassed', polkadot);
  const observeSchedulerDispatched = observeEvent('scheduler:Dispatched', polkadot);
  const vote = { Standard: { vote: true, balance: amount } };
  await submitAndGetEvent(
    polkadot.tx.democracy.vote(proposalIndex, vote),
    polkadot.events.democracy.Voted,
  );
  console.log(`voted for proposal ${proposalIndex}`);

  // Wait for it to pass
  await Promise.race([observeDemocracyPassed, observeDemocracyNotPassed])
    .then((event) => {
      if (event.name.method !== 'Passed') {
        throw new Error(`Democracy failed for runtime upgrade. ${proposalIndex}`);
      }
    })
    .catch((error) => {
      console.error(error);
      process.exit(-1);
    });
  console.log('Democracy manifest! waiting for a succulent scheduled runtime upgrade...');

  // Wait for the runtime upgrade to complete
  const schedulerDispatchedEvent = await observeSchedulerDispatched;
  if (schedulerDispatchedEvent.data.result.Err) {
    console.log('Runtime upgrade failed');
    handleDispatchError({
      dispatchError: JSON.stringify({ module: schedulerDispatchedEvent.data.result.Err.Module }),
    });
    process.exit(-1);
  }
  console.log('-- Polkadot runtime upgrade complete --');
}

async function randomPolkadotSwap(): Promise<void> {
  const assets: Asset[] = [Assets.BTC, Assets.ETH, Assets.USDC, Assets.FLIP];
  const randomAsset = assets[Math.floor(Math.random() * assets.length)];

  let sourceAsset: Asset;
  let destAsset: Asset;

  if (Math.random() < 0.5) {
    sourceAsset = Assets.DOT;
    destAsset = randomAsset;
  } else {
    sourceAsset = randomAsset;
    destAsset = Assets.DOT;
  }

  await testSwap(sourceAsset, destAsset, undefined, undefined, undefined, undefined, false);
  swapsComplete++;
}

async function doPolkadotSwaps(): Promise<void> {
  console.log('Running polkadot, new swap every 1 second');
  while (!runtimeUpgradeComplete) {
    randomPolkadotSwap();
    swapsStarted++;
    await sleep(1000);
  }
  console.log(`Stopping polkadot swaps, ${swapsComplete}/${swapsStarted} swaps complete.`);

  // Wait for all of the swaps to complete
  while (swapsComplete < swapsStarted) {
    await sleep(1000);
  }
  console.log(`All ${swapsComplete} swaps complete`);
}

async function main(): Promise<void> {
  // Get polkadot source using git
  // tmp/ is ignored in the bouncer .gitignore file.
  const workspacePath = path.join(process.cwd(), 'tmp/polkadot');
  console.log('cloning polkadot repo to: ' + workspacePath);
  // TODO: this is not a great solution, what if the folder exists but is empty?
  if (!fs.existsSync(workspacePath)) {
    execSync(`git clone https://github.com/chainflip-io/polkadot.git ${workspacePath}`);
  }
  execSync(`cd ${workspacePath} && git reset --hard HEAD && git clean -fd && git pull`);

  // Bump the spec version
  const expectedSpecVersion = await bumpSpecVersionAgainstNetwork(
    `${workspacePath}/runtime/polkadot/src/lib.rs`,
    POLKADOT_ENDPOINT_PORT,
  );

  // Compile polkadot runtime
  console.log('Compiling polkadot...');
  execSync(`cd ${workspacePath} && cargo build --locked --release --features fast-runtime`);
  console.log('Finished compiling polkadot');
  const wasmPath = `${workspacePath}/target/release/wbuild/polkadot-runtime/polkadot_runtime.compact.compressed.wasm`;
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`Wasm file not found at ${wasmPath}`);
  }

  const swapping = doPolkadotSwaps();
  console.log('Waiting for swaps to start...');
  while (swapsComplete === 0) {
    await sleep(1000);
  }

  // Submit the runtime upgrade
  await pushPolkadotRuntimeUpgrade(wasmPath);
  runtimeUpgradeComplete = true;

  // Check the polkadot spec version has changed
  const postUpgradeSpecVersion = await getCurrentRuntimeVersion(POLKADOT_ENDPOINT_PORT);
  if (postUpgradeSpecVersion.specVersion !== expectedSpecVersion) {
    throw new Error(
      `Polkadot runtime upgrade failed. Currently at version ${postUpgradeSpecVersion.specVersion}, expected to be at ${expectedSpecVersion}`,
    );
  }

  // Wait for all of the swaps to complete
  console.log('Waiting for swaps to complete...');
  // FIXME: API is glitching out here with "API/INIT: Runtime version updated to spec=10002, tx=24".
  await swapping;

  process.exit(0);
}

runWithTimeout(main(), 500000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
