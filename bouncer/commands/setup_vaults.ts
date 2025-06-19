#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { AddressOrPair } from '@polkadot/api/types';
import Web3 from 'web3';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import {
  getBtcClient,
  handleSubstrateError,
  getEvmEndpoint,
  getSolConnection,
  deferredPromise,
} from '../shared/utils';
import { aliceKeyringPair } from '../shared/polkadot_keyring';
import {
  initializeArbitrumChain,
  initializeArbitrumContracts,
  initializeSolanaChain,
  initializeSolanaPrograms,
  initializeAssethubChain,
} from '../shared/initialize_new_chains';
import { globalLogger, loggerChild, Logger } from '../shared/utils/logger';
import {
  getPolkadotApi,
  getAssethubApi,
  observeEvent,
  DisposableApiPromise,
} from '../shared/utils/substrate';
import { brokerApiEndpoint, lpApiEndpoint } from '../shared/json_rpc';
import { updatePriceFeed } from '../shared/update_price_feed';
import { price } from '../shared/setup_swaps';

async function createPolkadotVault(logger: Logger, api: DisposableApiPromise) {
  const { promise, resolve } = deferredPromise<{
    vaultAddress: AddressOrPair;
    vaultExtrinsicIndex: number;
  }>();

  const alice = await aliceKeyringPair();
  const unsubscribe = await api.tx.proxy
    .createPure(api.createType('ProxyType', 'Any'), 0, 0)
    .signAndSend(alice, { nonce: -1 }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isInBlock) {
        logger.info('Polkadot Vault created');
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

  const unsubscribe = await api.tx.utility
    .batchAll([
      // Note the vault needs to be funded before we rotate.
      api.tx.balances.transferKeepAlive(vault, 1000000000000),
      api.tx.balances.transferKeepAlive(key, 1000000000000),
      rotation,
    ])
    .signAndSend(alice, { nonce: -1 }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isInBlock) {
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

  await using polkadot = await getPolkadotApi();
  await using assethub = await getAssethubApi();

  logger.info(`LP endpoint set to: ${lpApiEndpoint}`);
  logger.info(`Broker endpoint set to: ${brokerApiEndpoint}`);

  logger.info('Performing initial Vault setup');

  // Step 1
  await initializeArbitrumChain(logger);
  await initializeSolanaChain(logger);
  await initializeAssethubChain(logger);

  // Step 2
  logger.info('Forcing rotation');
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Step 3
  logger.info('Waiting for new keys');

  const dotActivationRequest = observeEvent(
    logger,
    'polkadotVault:AwaitingGovernanceActivation',
  ).event;
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

  const dotKey = (await dotActivationRequest).data.newPublicKey;
  const btcKey = (await btcActivationRequest).data.newPublicKey;
  const arbKey = (await arbActivationRequest).data.newPublicKey;
  const solKey = (await solActivationRequest).data.newPublicKey;
  const hubKey = (await hubActivationRequest).data.newPublicKey;

  // Step 4
  logger.info('Requesting Polkadot Vault creation');
  const { vaultAddress: dotVaultAddress } = await createPolkadotVault(logger, polkadot);
  const dotProxyAdded = observeEvent(logger, 'proxy:ProxyAdded', { chain: 'polkadot' }).event;

  logger.info('Requesting Assethub Vault creation');
  const { vaultAddress: hubVaultAddress } = await createPolkadotVault(logger, assethub);
  const hubProxyAdded = observeEvent(logger, 'proxy:ProxyAdded', { chain: 'assethub' }).event;

  // Step 5
  logger.info('Rotating Proxy and Funding Accounts on Polkadot and Assethub');
  const [, , dotVaultEvent, hubVaultEvent] = await Promise.all([
    rotateAndFund(polkadot, dotVaultAddress, dotKey),
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    dotProxyAdded,
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
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessPolkadotVaultCreation(dotVaultAddress, {
      blockNumber: dotVaultEvent.block,
      extrinsicIndex: dotVaultEvent.eventIndex,
    }),
  );
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
      blockNumber: hubVaultEvent.block,
      extrinsicIndex: hubVaultEvent.eventIndex,
    }),
  );
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
      await btcClient.getBlockCount(),
      btcKey,
    ),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessInitializeSolanaVault(await solClient.getSlot()),
  );

  // Step 8
  logger.info('Setting up price feeds');
  await updatePriceFeed(logger, 'Ethereum', 'BTC', price.get('Btc')!.toString());
  await updatePriceFeed(logger, 'Ethereum', 'ETH', price.get('Eth')!.toString());
  await updatePriceFeed(logger, 'Ethereum', 'SOL', price.get('Sol')!.toString());
  await updatePriceFeed(logger, 'Solana', 'BTC', price.get('Btc')!.toString());
  await updatePriceFeed(logger, 'Solana', 'ETH', price.get('Eth')!.toString());
  await updatePriceFeed(logger, 'Solana', 'SOL', price.get('Sol')!.toString());

  // Confirmation
  logger.info('Waiting for new epoch...');
  await observeEvent(logger, 'validator:NewEpoch', {
    historicalCheckBlocks: 10,
  }).event;

  logger.info('New Epoch');
  logger.info('Vault Setup completed');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
