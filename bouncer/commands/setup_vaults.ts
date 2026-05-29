#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { getBtcClient, getSolConnection, getWeb3, getTronWebClient } from 'shared/utils';
import {
  initializeArbitrumChain,
  initializeArbitrumContracts,
  initializeSolanaChain,
  initializeSolanaPrograms,
  initializeTronChain,
  initializeTronContracts,
} from 'shared/initialize_new_chains';
import { globalLogger, loggerChild } from 'shared/utils/logger';
import { brokerApiEndpoint, lpApiEndpoint } from 'shared/json_rpc';
import { updateDefaultPriceFeeds } from 'shared/update_price_feed';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { bitcoinVaultAwaitingGovernanceActivationEvent } from 'generated/events/bitcoinVault/awaitingGovernanceActivation';
import { arbitrumVaultAwaitingGovernanceActivationEvent } from 'generated/events/arbitrumVault/awaitingGovernanceActivation';
import { solanaVaultAwaitingGovernanceActivationEvent } from 'generated/events/solanaVault/awaitingGovernanceActivation';
import { tronVaultAwaitingGovernanceActivationEvent } from 'generated/events/tronVault/awaitingGovernanceActivation';
import { validatorNewEpochEvent } from 'generated/events/validator/newEpoch';
import { validatorRotationPhaseUpdatedEvent } from 'generated/events/validator/rotationPhaseUpdated';
import { validatorRotationAbortedEvent } from 'generated/events/validator/rotationAborted';
import { environmentBitcoinBlockNumberSetForVaultEvent } from 'generated/events/environment/bitcoinBlockNumberSetForVault';
import { environmentSolanaInitializedEvent } from 'generated/events/environment/solanaInitialized';

async function main(): Promise<void> {
  const cf = await newChainflipIO(loggerChild(globalLogger, 'setup_vaults'), []);
  const btcClient = getBtcClient();
  const arbClient = getWeb3('Arbitrum');
  const solClient = getSolConnection();
  const tronClient = getTronWebClient();

  cf.info(`LP endpoint set to: ${lpApiEndpoint}`);
  cf.info(`Broker endpoint set to: ${brokerApiEndpoint}`);

  cf.info('Performing initial Vault setup');

  // Step 1
  await Promise.all([
    initializeArbitrumChain(cf.logger),
    initializeSolanaChain(cf.logger),
    initializeTronChain(cf.logger),
  ]);

  // Step 2
  cf.info('Forcing rotation');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });

  const rotationEvent = await cf.stepUntilOneEventOf({
    rotationPhaseUpdated: validatorRotationPhaseUpdatedEvent.refine(
      (event) => event.newPhase.__kind === 'KeygensInProgress',
    ),
    rotationAborted: validatorRotationAbortedEvent,
  });
  if (rotationEvent.key === 'rotationAborted') {
    throw new Error(
      `Initial setup_vaults forced rotation was ABORTED. Cannot continue with the test, please check the node logs for possible reasons.`,
    );
  }

  // Step 3
  cf.info('Waiting for new keys');
  const keyEvents = await cf.stepUntilAllEventsOf({
    btc: bitcoinVaultAwaitingGovernanceActivationEvent,
    arb: arbitrumVaultAwaitingGovernanceActivationEvent,
    sol: solanaVaultAwaitingGovernanceActivationEvent,
    tron: tronVaultAwaitingGovernanceActivationEvent,
  });

  const btcKey = keyEvents.btc.data.newPublicKey;
  const arbKey = keyEvents.arb.data.newPublicKey;
  const solKey = keyEvents.sol.data.newPublicKey;
  const tronKey = keyEvents.tron.data.newPublicKey;

  // Step 4
  cf.info('Setting up external chains (Arbitrum, Solana, Tron) with new keys');

  const insertArbitrumKey = async () => {
    cf.info('Inserting Arbitrum key in the contracts');
    await initializeArbitrumContracts(cf.logger, arbClient, arbKey);
    cf.debug('Arbitrum key inserted');
  };

  const insertSolanaKey = async () => {
    cf.info('Inserting Solana key in the programs');
    await initializeSolanaPrograms(cf.logger, solKey);
    cf.debug('Solana key inserted');
  };

  const insertTronKey = async () => {
    cf.info('Inserting Tron key in the contracts');
    await initializeTronContracts(tronClient, tronKey);
    cf.debug('Tron key inserted');
  };

  await Promise.all([insertArbitrumKey(), insertSolanaKey(), insertTronKey()]);

  // Step 7
  cf.info('Setting up price feeds');
  const updateDefaultPriceFeedsHandle = updateDefaultPriceFeeds(cf.logger);

  // Step 8
  cf.info('Registering Vaults with state chain');

  await cf.all([
    (subcf) =>
      subcf.submitGovernance({
        extrinsic: async (api) =>
          api.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
            await btcClient.getBlockCount(),
            btcKey,
          ),
        expectedEvent: environmentBitcoinBlockNumberSetForVaultEvent,
      }),
    (subcf) =>
      subcf.submitGovernance({
        extrinsic: async (api) =>
          api.tx.environment.witnessInitializeSolanaVault(await solClient.getSlot()),
        expectedEvent: environmentSolanaInitializedEvent,
      }),
  ]);

  // Confirmation
  cf.info('Waiting for new epoch...');
  await cf.stepUntilEvent(validatorNewEpochEvent);
  cf.info('New Epoch');

  // Wait for updateDefaultPriceFeeds Promise to resolve
  await updateDefaultPriceFeedsHandle;

  cf.info('Vault Setup completed');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
