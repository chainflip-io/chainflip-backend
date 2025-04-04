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
} from '../shared/initialize_new_chains';
import { globalLogger, loggerChild } from '../shared/utils/logger';
import { getPolkadotApi, observeEvent } from '../shared/utils/substrate';
import { brokerApiEndpoint, lpApiEndpoint } from '../shared/json_rpc';

async function main(): Promise<void> {
  const logger = loggerChild(globalLogger, 'setup_vaults');
  const btcClient = getBtcClient();
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));
  const alice = await aliceKeyringPair();
  const solClient = getSolConnection();

  await using polkadot = await getPolkadotApi();

  logger.info(`LP endpoint set to: ${lpApiEndpoint}`);
  logger.info(`Broker endpoint set to: ${brokerApiEndpoint}`);

  logger.info('Performing initial Vault setup');

  // Step 1
  await initializeArbitrumChain(logger);
  await initializeSolanaChain(logger);

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
  const dotKey = (await dotActivationRequest).data.newPublicKey;
  const btcKey = (await btcActivationRequest).data.newPublicKey;
  const arbKey = (await arbActivationRequest).data.newPublicKey;
  const solKey = (await solActivationRequest).data.newPublicKey;

  // Step 4
  logger.info('Requesting Polkadot Vault creation');
  const createPolkadotVault = async () => {
    const { promise, resolve } = deferredPromise<{
      vaultAddress: AddressOrPair;
      vaultExtrinsicIndex: number;
    }>();

    const unsubscribe = await polkadot.tx.proxy
      .createPure(polkadot.createType('ProxyType', 'Any'), 0, 0)
      .signAndSend(alice, { nonce: -1 }, (result) => {
        if (result.isError) {
          handleSubstrateError(polkadot)(result);
        }
        if (result.isInBlock) {
          logger.info('Polkadot Vault created');
          const pureCreated = result.findRecord('proxy', 'PureCreated')!;
          resolve({
            vaultAddress: pureCreated.event.data[0] as AddressOrPair,
            vaultExtrinsicIndex: result.txIndex!,
          });
          unsubscribe();
        }
      });

    return promise;
  };
  const { vaultAddress, vaultExtrinsicIndex } = await createPolkadotVault();

  const proxyAdded = observeEvent(logger, 'proxy:ProxyAdded', { chain: 'polkadot' }).event;

  // Step 5
  logger.info('Rotating Proxy and Funding Accounts.');
  const rotateAndFund = async () => {
    const { promise, resolve } = deferredPromise<void>();
    const rotation = polkadot.tx.proxy.proxy(
      polkadot.createType('MultiAddress', vaultAddress),
      null,
      polkadot.tx.utility.batchAll([
        polkadot.tx.proxy.addProxy(
          polkadot.createType('MultiAddress', dotKey),
          polkadot.createType('ProxyType', 'Any'),
          0,
        ),
        polkadot.tx.proxy.removeProxy(
          polkadot.createType('MultiAddress', alice.address),
          polkadot.createType('ProxyType', 'Any'),
          0,
        ),
      ]),
    );

    const unsubscribe = await polkadot.tx.utility
      .batchAll([
        // Note the vault needs to be funded before we rotate.
        polkadot.tx.balances.transferKeepAlive(vaultAddress, 1000000000000),
        polkadot.tx.balances.transferKeepAlive(dotKey, 1000000000000),
        rotation,
      ])
      .signAndSend(alice, { nonce: -1 }, (result) => {
        if (result.isError) {
          handleSubstrateError(polkadot)(result);
        }
        if (result.isInBlock) {
          unsubscribe();
          resolve();
        }
      });

    await promise;
  };
  await rotateAndFund();
  const vaultBlockNumber = (await proxyAdded).block;

  // Step 6
  logger.info('Inserting Arbitrum key in the contracts');
  await initializeArbitrumContracts(logger, arbClient, arbKey);

  // Using arbitrary key for now, we will use solKey generated by SC
  logger.info('Inserting Solana key in the programs');
  await initializeSolanaPrograms(logger, solClient, solKey);

  // Step 7
  logger.info('Registering Vaults with state chain');
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessPolkadotVaultCreation(vaultAddress, {
      blockNumber: vaultBlockNumber,
      extrinsicIndex: vaultExtrinsicIndex,
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

  // Confirmation
  logger.info('Waiting for new epoch...');
  await observeEvent(logger, 'validator:NewEpoch').event;

  logger.info('New Epoch');
  logger.info('Vault Setup completed');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
