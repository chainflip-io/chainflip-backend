import fs from 'fs';
import path from 'path';
import assert from 'assert';
import { execSync } from 'child_process';

import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { blake2AsU8a } from '../polkadot/util-crypto';
import { amountToFineAmount, sleep, assetDecimals } from '../shared/utils';
import { specVersion, getNetworkRuntimeVersion } from '../shared/utils/spec_version';
import { handleDispatchError, submitAndGetEvent } from '../shared/polkadot_utils';
import { testSwap } from '../shared/swapping';
import { observeEvent, observeBadEvent, getPolkadotApi } from '../shared/utils/substrate';
import { Logger, loggerChild } from '../shared/utils/logger';
import { TestContext } from '../shared/utils/test_context';

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.

const POLKADOT_REPO_URL = `https://github.com/chainflip-io/polkadot.git`;
const PROPOSAL_AMOUNT = '100';
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
export async function pushPolkadotRuntimeUpdate(logger: Logger, wasmPath: string): Promise<void> {
  await using polkadot = await getPolkadotApi();
  logger.info('-- Pushing polkadot runtime update --');

  // Read the runtime wasm from file
  const runtimeWasm = fs.readFileSync(wasmPath);
  if (runtimeWasm.length > 4194304) {
    throw new Error(`runtimeWasm file too large (${runtimeWasm.length}b), must be less than 4mb`);
  }

  // Submit the preimage (if it doesn't already exist)
  const setCodeCall = polkadot.tx.system.setCode(Array.from(runtimeWasm));
  const preimage = setCodeCall.method.toHex();
  const preimageHash = '0x' + Buffer.from(blake2AsU8a(preimage)).toString('hex');
  logger.debug(`Preimage hash: ${preimageHash}`);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let preimageStatus = (await polkadot.query.preimage.statusFor(preimageHash)) as any;
  if (JSON.stringify(preimageStatus) !== 'null') {
    preimageStatus = JSON.parse(preimageStatus);
    if (!preimageStatus?.unrequested && !preimageStatus?.requested) {
      throw new Error('Invalid preimage status');
    }
  }
  if (preimageStatus?.unrequested?.len > 0 || preimageStatus?.requested?.len > 0) {
    logger.debug('Preimage already exists, skipping submission');
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
    logger.debug(`Preimage submitted: ${preimageHash}`);
  }

  // Submit the proposal
  const observeDemocracyStarted = observeEvent(logger, 'democracy:Started', {
    chain: 'polkadot',
  }).event;
  const amount = amountToFineAmount(PROPOSAL_AMOUNT, assetDecimals('Dot'));
  logger.debug(`Submitting proposal with amount: ${PROPOSAL_AMOUNT}`);
  const democracyStartedEvent = await submitAndGetEvent(
    polkadot.tx.democracy.propose({ Legacy: preimageHash }, amount),
    polkadot.events.democracy.Proposed,
  );
  const proposalIndex = democracyStartedEvent.data[0];
  logger.debug(`proposal submitted: index ${proposalIndex}`);

  // Wait for the democracy started event
  logger.debug('Waiting for voting to start...');
  await observeDemocracyStarted;

  // Vote for the proposal
  const observeDemocracyPassed = observeEvent(logger, 'democracy:Passed', {
    chain: 'polkadot',
  }).event;
  const observeDemocracyNotPassed = observeEvent(logger, 'democracy:NotPassed', {
    chain: 'polkadot',
  }).event;
  const observeSchedulerDispatched = observeEvent(logger, 'scheduler:Dispatched', {
    chain: 'polkadot',
  }).event;
  const observeCodeUpdated = observeEvent(logger, 'system:CodeUpdated', {
    chain: 'polkadot',
  }).event;
  const vote = { Standard: { vote: true, balance: amount } };
  await submitAndGetEvent(
    polkadot.tx.democracy.vote(proposalIndex, vote),
    polkadot.events.democracy.Voted,
  );
  logger.debug(`voted for proposal ${proposalIndex}`);

  // Stopping swaps now because the api sometimes gets error 1010 (bad signature) when depositing dot after the runtime update but before the api is updated.
  runtimeUpdatePushed = true;

  // Wait for it to pass
  await Promise.race([observeDemocracyPassed, observeDemocracyNotPassed]).then((event) => {
    if (event.name.method !== 'Passed') {
      throw new Error(`Democracy failed for runtime update. ${proposalIndex}`);
    }
  });
  logger.debug('Democracy manifest! waiting for a succulent scheduled runtime update...');

  // Wait for the runtime update to complete
  const schedulerDispatchedEvent = await observeSchedulerDispatched;
  if (schedulerDispatchedEvent.data.result.Err) {
    logger.debug('Runtime update failed');
    await handleDispatchError({
      dispatchError: JSON.stringify({ module: schedulerDispatchedEvent.data.result.Err.Module }),
    });
    process.exit(-1);
  }
  logger.debug(`Scheduler dispatched Runtime update at block ${schedulerDispatchedEvent.block}`);

  const CodeUpdated = await observeCodeUpdated;
  logger.debug(`Code updated at block ${CodeUpdated.block}`);

  logger.info('-- Polkadot runtime update complete --');
}

/// Pulls the polkadot source code and bumps the spec version, then compiles it if necessary.
/// If the bumped spec version matches the pre-compiled one stored in the repo, then it will use that instead.
export async function bumpAndBuildPolkadotRuntime(logger: Logger): Promise<[string, number]> {
  const projectPath = process.cwd();
  // tmp/ is ignored in the bouncer .gitignore file.
  const workspacePath = path.join(projectPath, 'tmp/polkadot');
  const nextSpecVersion =
    (await getNetworkRuntimeVersion(logger, polkadotEndpoint)).specVersion + 1;
  logger.debug('Current polkadot spec_version: ' + nextSpecVersion);

  // No need to compile if the version we need is the pre-compiled version.
  const preCompiledWasmPath = `${projectPath}/tests/test_data/polkadot_runtime_${PRE_COMPILED_WASM_VERSION}.wasm`;
  let copyToPreCompileLocation = false;
  if (nextSpecVersion === PRE_COMPILED_WASM_VERSION) {
    if (!fs.existsSync(preCompiledWasmPath)) {
      logger.debug(
        `Warning: Precompiled Wasm file not found at "${preCompiledWasmPath}". It will be compiled and copied there. You will need to commit the file to the repo to speed up future runs.`,
      );
      copyToPreCompileLocation = true;
    } else {
      logger.debug(`Using pre-compiled wasm file: ${preCompiledWasmPath}`);
      return [preCompiledWasmPath, nextSpecVersion];
    }
  }

  // Get polkadot source using git
  if (!fs.existsSync(workspacePath)) {
    logger.debug('Cloning polkadot repo to: ' + workspacePath);
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
  logger.debug('Updating polkadot source');
  execSync(`git pull`, { cwd: workspacePath });

  specVersion(logger, `${workspacePath}/runtime/polkadot/src/lib.rs`, 'write', nextSpecVersion);

  // Compile polkadot runtime
  logger.debug('Compiling polkadot...');
  execSync(`cargo build --locked --release --features fast-runtime`, { cwd: workspacePath });
  logger.info('Finished compiling polkadot');
  const wasmPath = `${workspacePath}/target/release/wbuild/polkadot-runtime/polkadot_runtime.compact.compressed.wasm`;
  if (!fs.existsSync(wasmPath)) {
    throw new Error(`Wasm file not found: ${wasmPath}`);
  }

  // Backup the pre-compiled wasm file so we do not have to build it again on future fresh runs.
  if (copyToPreCompileLocation) {
    fs.copyFileSync(wasmPath, preCompiledWasmPath);
    logger.debug(`Copied ${wasmPath} to ${preCompiledWasmPath}`);
  }

  return [wasmPath, nextSpecVersion];
}

async function randomPolkadotSwap(testContext: TestContext): Promise<void> {
  const assets: Asset[] = [Assets.Btc, Assets.Eth, Assets.Usdc, Assets.Flip];
  const randomAsset = assets[Math.floor(Math.random() * assets.length)];

  let sourceAsset: Asset;
  let destAsset: Asset;

  if (Math.random() < 0.5) {
    sourceAsset = Assets.Dot;
    destAsset = randomAsset;
  } else {
    sourceAsset = randomAsset;
    destAsset = Assets.Dot;
  }

  await testSwap(
    testContext.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    testContext.swapContext,
    undefined,
    undefined,
  );
  swapsComplete++;
  testContext.debug(`Swap complete: (${swapsComplete}/${swapsStarted})`);
}

async function doPolkadotSwaps(testContext: TestContext): Promise<void> {
  const logger = loggerChild(testContext.logger, 'doPolkadotSwaps');
  const startSwapInterval = 2000;
  logger.debug(`Running polkadot swaps, new random swap every ${startSwapInterval}ms`);
  while (!runtimeUpdatePushed) {
    /* eslint-disable @typescript-eslint/no-floating-promises */
    randomPolkadotSwap(testContext);
    swapsStarted++;
    await sleep(startSwapInterval);
  }
  logger.debug(`Stopping polkadot swaps, ${swapsComplete}/${swapsStarted} swaps complete.`);

  // Wait for all of the swaps to complete
  while (swapsComplete < swapsStarted) {
    await sleep(1000);
  }
  logger.info(`All ${swapsComplete} swaps complete`);
}

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.
export async function testPolkadotRuntimeUpdate(testContext: TestContext): Promise<void> {
  const logger = testContext.logger;
  const [wasmPath, expectedSpecVersion] = await bumpAndBuildPolkadotRuntime(logger);

  // Monitor for the broadcast aborted event to help catch failed swaps
  const broadcastAborted = observeBadEvent(logger, ':BroadcastAborted', {});

  // Start some swaps
  const swapping = doPolkadotSwaps(testContext);
  logger.debug('Waiting for swaps to start...');
  while (swapsComplete === 0) {
    await sleep(1000);
  }

  // Submit the runtime update
  await pushPolkadotRuntimeUpdate(logger, wasmPath);

  // Check the polkadot spec version has changed
  const postUpgradeSpecVersion = await getNetworkRuntimeVersion(logger, polkadotEndpoint);
  if (postUpgradeSpecVersion.specVersion !== expectedSpecVersion) {
    throw new Error(
      `Polkadot runtime update failed. Currently at version ${postUpgradeSpecVersion.specVersion}, expected to be at ${expectedSpecVersion}`,
    );
  }

  // Wait for all of the swaps to complete
  logger.info('Waiting for swaps to complete...');
  await swapping;
  await broadcastAborted.stop();
}
