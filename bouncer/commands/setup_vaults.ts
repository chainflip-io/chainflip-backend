#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { AddressOrPair } from '@polkadot/api/types';
import Web3 from 'web3';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import {
  getBtcClient,
  handleSubstrateError,
  getEvmEndpoint,
  getSolConnection,
  deferredPromise,
} from 'shared/utils';
import { aliceKeyringPair } from 'shared/polkadot_keyring';
import {
  initializeArbitrumChain,
  initializeArbitrumContracts,
  initializeSolanaChain,
  initializeSolanaPrograms,
  initializeAssethubChain,
} from 'shared/initialize_new_chains';
import { globalLogger, loggerChild } from 'shared/utils/logger';
import { getAssethubApi, observeEvent, DisposableApiPromise } from 'shared/utils/substrate';
import { brokerApiEndpoint, lpApiEndpoint } from 'shared/json_rpc';
import { updateDefaultPriceFeeds } from 'shared/update_price_feed';

export async function createPolkadotVault(api: DisposableApiPromise) {
  const { promise, resolve } = deferredPromise<{
    vaultAddress: AddressOrPair;
    vaultExtrinsicIndex: number;
  }>();

  const alice = await aliceKeyringPair();
  const nonce = await api.rpc.system.accountNextIndex(alice.address);
  const unsubscribe = await api.tx.proxy
    .createPure(api.createType('ProxyType', 'Any'), 0, 0)
    .signAndSend(alice, { nonce }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isFinalized) {
        // TODO: figure out type inference so we don't have to coerce using `any`
        const pureCreated = result.findRecord('proxy', 'PureCreated')!;
        resolve({
          vaultAddress: pureCreated.event.data[0] as AddressOrPair,
          vaultExtrinsicIndex: result.txIndex!,
        });
        unsubscribe();
      }
    });

  return promise;
}

async function rotateAndFund(api: DisposableApiPromise, vault: AddressOrPair, key: AddressOrPair) {
  const { promise, resolve } = deferredPromise<void>();
  const alice = await aliceKeyringPair();
  const rotation = api.tx.proxy.proxy(
    api.createType('MultiAddress', vault),
    null,
    api.tx.utility.batchAll([
      api.tx.proxy.addProxy(
        api.createType('MultiAddress', key),
        api.createType('ProxyType', 'Any'),
        0,
      ),
      api.tx.proxy.removeProxy(
        api.createType('MultiAddress', alice.address),
        api.createType('ProxyType', 'Any'),
        0,
      ),
    ]),
  );

  const nonce = await api.rpc.system.accountNextIndex(alice.address);
  const unsubscribe = await api.tx.utility
    .batchAll([
      // Note the vault needs to be funded before we rotate.
      api.tx.balances.transferKeepAlive(vault, 1000000000000),
      api.tx.balances.transferKeepAlive(key, 1000000000000),
      rotation,
    ])
    .signAndSend(alice, { nonce }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isFinalized) {
        unsubscribe();
        resolve();
      }
    });

  await promise;
}

async function main(): Promise<void> {
  const logger = loggerChild(globalLogger, 'setup_vaults');
  const btcClient = getBtcClient();
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));
  const solClient = getSolConnection();

  await using assethub = await getAssethubApi();

  logger.info(`LP endpoint set to: ${lpApiEndpoint}`);
  logger.info(`Broker endpoint set to: ${brokerApiEndpoint}`);

  logger.info('Performing initial Vault setup');

  // Step 1
  await Promise.all([
    initializeArbitrumChain(logger),
    initializeSolanaChain(logger),
    initializeAssethubChain(logger),
  ]);

  // Step 2
  const btcActivationRequest = observeEvent(
    logger,
    'bitcoinVault:AwaitingGovernanceActivation',
  ).event;
  const arbActivationRequest = observeEvent(
    logger,
    'arbitrumVault:AwaitingGovernanceActivation',
  ).event;
  const solActivationRequest = observeEvent(
    logger,
    'solanaVault:AwaitingGovernanceActivation',
  ).event;
  const hubActivationRequest = observeEvent(
    logger,
    'assethubVault:AwaitingGovernanceActivation',
  ).event;
  logger.info('Forcing rotation');
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Step 3
  logger.info('Waiting for new keys');

  const btcKey = (await btcActivationRequest).data.newPublicKey;
  const arbKey = (await arbActivationRequest).data.newPublicKey;
  const solKey = (await solActivationRequest).data.newPublicKey;
  const hubKey = (await hubActivationRequest).data.newPublicKey;

  // Step 4
  logger.info('Requesting Assethub Vault creation');
  const { vaultAddress: hubVaultAddress } = await createPolkadotVault(assethub);
  logger.info(`AssetHub vault created, address: ${hubVaultAddress}`);

  // Step 5
  const hubProxyAdded = observeEvent(logger, 'proxy:ProxyAdded', {
    chain: 'assethub',
    timeoutSeconds: 120,
  }).event;
  logger.info('Rotating Proxy and Funding Accounts on Assethub');
  const [, hubVaultEvent] = await Promise.all([
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    hubProxyAdded,
  ]);

  // Step 6
  logger.info('Inserting Arbitrum key in the contracts');
  await initializeArbitrumContracts(logger, arbClient, arbKey);

  // Using arbitrary key for now, we will use solKey generated by SC
  logger.info('Inserting Solana key in the programs');
  await initializeSolanaPrograms(logger, solKey);

  // Step 7
  logger.info('Registering Vaults with state chain');

  const assethubVaultCreatedEvent = observeEvent(
    logger,
    'assethubVault:VaultActivationCompleted',
  ).event;
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
      blockNumber: hubVaultEvent.block,
      extrinsicIndex: hubVaultEvent.eventIndex,
    }),
  );
  await assethubVaultCreatedEvent;

  const bitcoinBlocknumberSetEvent = observeEvent(
    logger,
    'environment:BitcoinBlockNumberSetForVault',
  ).event;
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
      await btcClient.getBlockCount(),
      btcKey,
    ),
  );
  await bitcoinBlocknumberSetEvent;

  const arbitrumInitializedEvent = observeEvent(logger, 'environment:ArbitrumInitialized').event;
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );
  await arbitrumInitializedEvent;

  const solanaInitializedEvent = observeEvent(logger, 'environment:SolanaInitialized').event;
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessInitializeSolanaVault(await solClient.getSlot()),
  );
  await solanaInitializedEvent;

  // Step 8
  logger.info('Setting up price feeds');
  await updateDefaultPriceFeeds(logger);

  // Confirmation
  logger.info('Waiting for new epoch...');
  await observeEvent(logger, 'validator:NewEpoch', {
    historicalCheckBlocks: 100,
  }).event;

  logger.info('New Epoch');
  logger.info('Vault Setup completed');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
