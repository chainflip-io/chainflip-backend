import fs from 'fs';
import path from 'path';
import assert from 'assert';
import { execSync } from 'child_process';

import { blake2AsU8a } from '@polkadot/util-crypto';
import { Asset, Assets, assetDecimals } from '@chainflip-io/cli';
import {
  getPolkadotApi,
  observeEvent,
  amountToFineAmount,
  sleep,
  observeBadEvents,
} from '../shared/utils';
import { specVersion, getNetworkRuntimeVersion } from './utils/spec_version';
import { handleDispatchError, submitAndGetEvent } from '../shared/polkadot_utils';
import { testSwap } from './swapping';

const POLKADOT_REPO_URL = `https://github.com/chainflip-io/polkadot.git`;
const PROPOSAL_AMOUNT = '100';
const polkadot = await getPolkadotApi();
const polkadotEndpoint = 'http://127.0.0.1:9947';

// The spec version of the runtime wasm file that is in the repo at bouncer/test_data/polkadot_runtime_xxxx.wasm
// When the localnet polkadot runtime version is updated, change this value to be +1 and this test will compile the new wasm file for you.
// Then you will need to delete the old file and commit the new one.
const PRE_COMPILED_WASM_VERSION = 10001;

/// The update is sent to the polkadot chain.
let runtimeUpdatePushed = false;
let swapsComplete = 0;
let swapsStarted = 0;

/// Pushes a polkadot runtime update using the democracy pallet.
/// preimage -> proposal -> vote -> democracy pass -> scheduler dispatch runtime update.
export async function pushPolkadotRuntimeUpdate(wasmPath: string): Promise<void> {
  console.log('-- Pushing polkadot runtime update --');

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
  await observeDemocracyStarted;

  // Vote for the proposal
  const observeDemocracyPassed = observeEvent('democracy:Passed', polkadot);
  const observeDemocracyNotPassed = observeEvent('democracy:NotPassed', polkadot);
  const observeSchedulerDispatched = observeEvent('scheduler:Dispatched', polkadot);
  const observeCodeUpdated = observeEvent('system:CodeUpdated', polkadot);
  const vote = { Standard: { vote: true, balance: amount } };
  await submitAndGetEvent(
    polkadot.tx.democracy.vote(proposalIndex, vote),
    polkadot.events.democracy.Voted,
  );
  console.log(`voted for proposal ${proposalIndex}`);

  // Stopping swaps now because the api sometimes gets error 1010 (bad signature) when depositing dot after the runtime update but before the api is updated.
  runtimeUpdatePushed = true;

  // Wait for it to pass
  await Promise.race([observeDemocracyPassed, observeDemocracyNotPassed])
    .then((event) => {
      if (event.name.method !== 'Passed') {
        throw new Error(`Democracy failed for runtime update. ${proposalIndex}`);
      }
    })
    .catch((error) => {
      console.error(error);
      process.exit(-1);
    });
  console.log('Democracy manifest! waiting for a succulent scheduled runtime update...');

  // Wait for the runtime update to complete
  const schedulerDispatchedEvent = await observeSchedulerDispatched;
  if (schedulerDispatchedEvent.data.result.Err) {
    console.log('Runtime update failed');
    handleDispatchError({
      dispatchError: JSON.stringify({ module: schedulerDispatchedEvent.data.result.Err.Module }),
    });
    process.exit(-1);
  }
  console.log(`Scheduler dispatched Runtime update at block ${schedulerDispatchedEvent.block}`);

  const CodeUpdated = await observeCodeUpdated;
  console.log(`Code updated at block ${CodeUpdated.block}`);

  console.log('-- Polkadot runtime update complete --');
}

/// Pulls the polkadot source code and bumps the spec version, then compiles it if necessary.
/// If the bumped spec version matches the pre-compiled one stored in the repo, then it will use that instead.
export async function bumpAndBuildPolkadotRuntime(): Promise<[string, number]> {
  const projectPath = process.cwd();
  // tmp/ is ignored in the bouncer .gitignore file.
  const workspacePath = path.join(projectPath, 'tmp/polkadot');
  const nextSpecVersion = (await getNetworkRuntimeVersion(polkadotEndpoint)).specVersion + 1;
  console.log('Current polkadot spec_version: ' + nextSpecVersion);

  // No need to compile if the version we need is the pre-compiled version.
  const preCompiledWasmPath = `${projectPath}/tests/test_data/polkadot_runtime_${PRE_COMPILED_WASM_VERSION}.wasm`;
  let copyToPreCompileLocation = false;
  if (nextSpecVersion === PRE_COMPILED_WASM_VERSION) {
    if (!fs.existsSync(preCompiledWasmPath)) {
      console.log(
        `Warning: Precompiled Wasm file not found at "${preCompiledWasmPath}". It will be compiled and copied there. You will need to commit the file to the repo to speed up future runs.`,
      );
      copyToPreCompileLocation = true;
    } else {
      console.log(`Using pre-compiled wasm file: ${preCompiledWasmPath}`);
      return [preCompiledWasmPath, nextSpecVersion];
    }
  }

  // Get polkadot source using git
  if (!fs.existsSync(workspacePath)) {
    console.log('Cloning polkadot repo to: ' + workspacePath);
    execSync(`git clone https://github.com/chainflip-io/polkadot.git ${workspacePath}`);
  }
  const remoteUrl = execSync('git config --get remote.origin.url', { cwd: workspacePath })
    .toString()
    .trim();
  if (remoteUrl !== POLKADOT_REPO_URL) {
    throw new Error(
      `Polkadot folder exists at ${workspacePath} but is not the correct git repo: ${remoteUrl}. Please remove the folder and try again.`,
    );
  }
  console.log('Updating polkadot source');
  execSync(`git pull`, { cwd: workspacePath });

  await specVersion(`${workspacePath}/runtime/polkadot/src/lib.rs`, 'write', nextSpecVersion);

  // Compile polkadot runtime
  console.log('Compiling polkadot...');
  execSync(`cargo build --locked --release --features fast-runtime`, { cwd: workspacePath });
  console.log('Finished compiling polkadot');
  const wasmPath = `${workspacePath}/target/release/wbuild/polkadot-runtime/polkadot_runtime.compact.compressed.wasm`;
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`Wasm file not found: ${wasmPath}`);
  }

  // Backup the pre-compiled wasm file so we do not have to build it again on future fresh runs.
  if (copyToPreCompileLocation) {
    fs.copyFileSync(wasmPath, preCompiledWasmPath);
    console.log(`Copied ${wasmPath} to ${preCompiledWasmPath}`);
  }

  return [wasmPath, nextSpecVersion];
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
  console.log(`Swap complete: (${swapsComplete}/${swapsStarted})`);
}

async function doPolkadotSwaps(): Promise<void> {
  const startSwapInterval = 2000;
  console.log(`Running polkadot swaps, new random swap every ${startSwapInterval}ms`);
  while (!runtimeUpdatePushed) {
    randomPolkadotSwap();
    swapsStarted++;
    await sleep(startSwapInterval);
  }
  console.log(`Stopping polkadot swaps, ${swapsComplete}/${swapsStarted} swaps complete.`);

  // Wait for all of the swaps to complete
  while (swapsComplete < swapsStarted) {
    await sleep(1000);
  }
  console.log(`All ${swapsComplete} swaps complete`);
}

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.
export async function testPolkadotRuntimeUpdate(): Promise<void> {
  const [wasmPath, expectedSpecVersion] = await bumpAndBuildPolkadotRuntime();

  // Monitor for the broadcast aborted event to help catch failed swaps
  let stopObserving = false;
  const broadcastAborted = observeBadEvents(':BroadcastAborted', () => stopObserving);

  // Start some swaps
  const swapping = doPolkadotSwaps();
  console.log('Waiting for swaps to start...');
  while (swapsComplete === 0) {
    await sleep(1000);
  }

  // Submit the runtime update
  await pushPolkadotRuntimeUpdate(wasmPath);

  // Check the polkadot spec version has changed
  const postUpgradeSpecVersion = await getNetworkRuntimeVersion(polkadotEndpoint);
  if (postUpgradeSpecVersion.specVersion !== expectedSpecVersion) {
    throw new Error(
      `Polkadot runtime update failed. Currently at version ${postUpgradeSpecVersion.specVersion}, expected to be at ${expectedSpecVersion}`,
    );
  }

  // Wait for all of the swaps to complete
  console.log('Waiting for swaps to complete...');
  await swapping;
  stopObserving = true;
  await broadcastAborted;

  process.exit(0);
}
