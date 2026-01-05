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
import { newChainflipIO, } from 'shared/utils/chainflip_io';
import { bitcoinVaultAwaitingGovernanceActivation } from 'generated/events/bitcoinVault/awaitingGovernanceActivation';
import { arbitrumVaultAwaitingGovernanceActivation } from 'generated/events/arbitrumVault/awaitingGovernanceActivation';
import { solanaVaultAwaitingGovernanceActivation } from 'generated/events/solanaVault/awaitingGovernanceActivation';
import { assethubVaultAwaitingGovernanceActivation } from 'generated/events/assethubVault/awaitingGovernanceActivation';
import { environmentBitcoinBlockNumberSetForVault } from 'generated/events/environment/bitcoinBlockNumberSetForVault';
import { environmentArbitrumInitialized } from 'generated/events/environment/arbitrumInitialized';
import { assethubVaultVaultActivationCompleted } from 'generated/events/assethubVault/vaultActivationCompleted';
import { environmentSolanaInitialized } from 'generated/events/environment/solanaInitialized';
import { validatorNewEpoch } from 'generated/events/validator/newEpoch';
import { extrinsics } from '@polkadot/types/interfaces/definitions';

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
  const cf = await newChainflipIO(loggerChild(globalLogger, 'setup_vaults'), []);
  const btcClient = getBtcClient();
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));
  const solClient = getSolConnection();

  await using assethub = await getAssethubApi();

  cf.info(`LP endpoint set to: ${lpApiEndpoint}`);
  cf.info(`Broker endpoint set to: ${brokerApiEndpoint}`);

  cf.info('Performing initial Vault setup');

  // Step 1
  await Promise.all([
    initializeArbitrumChain(cf.logger),
    initializeSolanaChain(cf.logger),
    initializeAssethubChain(cf.logger),
  ]);

  // Step 2
  cf.info('Forcing rotation');
  await cf.submitGovernance({ extrinsic: (api) => api.tx.validator.forceRotation() });

  // Step 3
  cf.info('Waiting for new keys');
  const keyEvents = await cf.stepUntilAllEventsOf({
    btc: {
      name: 'BitcoinVault.AwaitingGovernanceActivation',
      schema: bitcoinVaultAwaitingGovernanceActivation,
    },
    arb: {
      name: 'ArbitrumVault.AwaitingGovernanceActivation',
      schema: arbitrumVaultAwaitingGovernanceActivation,
    },
    sol: {
      name: 'SolanaVault.AwaitingGovernanceActivation',
      schema: solanaVaultAwaitingGovernanceActivation,
    },
    hub: {
      name: 'AssethubVault.AwaitingGovernanceActivation',
      schema: assethubVaultAwaitingGovernanceActivation,
    },
  });

  const btcKey = keyEvents.btc.data.newPublicKey;
  const arbKey = keyEvents.arb.data.newPublicKey;
  const solKey = keyEvents.sol.data.newPublicKey;
  const hubKey = keyEvents.hub.data.newPublicKey;

  // Step 4
  cf.info('Setting up external chains (assethub, arbitrum, solana) with new keys')

  const createAssethubVault = async () => {
    // Step a
    cf.info('Requesting Assethub Vault creation');
    const { vaultAddress: hubVaultAddress } = await createPolkadotVault(assethub);
    cf.info(`AssetHub vault created, address: ${hubVaultAddress}`);

    // Step b
    const hubProxyAdded = observeEvent(cf.logger, 'proxy:ProxyAdded', {
      chain: 'assethub',
      timeoutSeconds: 120,
    }).event;
    cf.info('Rotating Proxy and Funding Accounts on Assethub');
    const [, hubVaultEvent] = await Promise.all([
      rotateAndFund(assethub, hubVaultAddress, hubKey),
      hubProxyAdded,
    ]);

    return { hubVaultAddress, hubVaultEvent };
  };

  const insertArbitrumKey = async () => {
    cf.info('Inserting Arbitrum key in the contracts');
    await initializeArbitrumContracts(cf.logger, arbClient, arbKey);
  };

  const insertSolanaKey = async () => {
    cf.info('Inserting Solana key in the programs');
    await initializeSolanaPrograms(cf.logger, solKey);
  };

  const [{ hubVaultAddress, hubVaultEvent }, a, b] = await Promise.all([
    createAssethubVault(),
    insertArbitrumKey(),
    insertSolanaKey(),
  ]);

  // Step 7
  cf.info('Registering Vaults with state chain');

  await cf.all([
    (cf) =>
      cf.submitGovernance({
        extrinsic: (api) =>
          api.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
            blockNumber: hubVaultEvent.block,
            extrinsicIndex: hubVaultEvent.eventIndex,
          }),
        expectedEvent: { name: 'AssethubVault.VaultActivationCompleted' },
      }),
    (cf) =>
      cf.submitGovernance({
        extrinsic: async (api) =>
          api.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
            await btcClient.getBlockCount(),
            btcKey,
          ),
        expectedEvent: { name: 'Environment.BitcoinBlockNumberSetForVault' },
      }),
    (cf) =>
      cf.submitGovernance({
        extrinsic: async (chainflip) =>
          chainflip.tx.environment.witnessInitializeArbitrumVault(
            await arbClient.eth.getBlockNumber(),
          ),
        expectedEvent: { name: 'Environment.ArbitrumInitialized' },
      }),
    (cf) =>
      cf.submitGovernance({
        extrinsic: async (chainflip) =>
          chainflip.tx.environment.witnessInitializeSolanaVault(await solClient.getSlot()),
        expectedEvent: { name: 'Environment.SolanaInitialized' },
      }),
  ]);

  // Step 8
  cf.info('Setting up price feeds');
  await updateDefaultPriceFeeds(cf.logger);

  // Confirmation
  cf.info('Waiting for new epoch...');
  await cf.stepUntilEvent('Validator.NewEpoch', validatorNewEpoch);

  cf.info('New Epoch');
  cf.info('Vault Setup completed');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
