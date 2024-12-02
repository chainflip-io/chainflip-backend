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
import { getPolkadotApi, getAssethubApi, observeEvent, DisposableApiPromise } from '../shared/utils/substrate';

async function createPolkadotVault(api: DisposableApiPromise) {
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
        console.log('Polkadot Vault created');
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
};

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
};

async function main(): Promise<void> {
  const btcClient = getBtcClient();
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));
  const solClient = getSolConnection();

  await using polkadot = await getPolkadotApi();
  await using assethub = await getAssethubApi();
  const alice = await aliceKeyringPair();

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  await initializeArbitrumChain();
  await initializeSolanaChain();
  await initializeAssethubChain();

  // Step 2
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Step 3
  console.log('Waiting for new keys');

  const dotActivationRequest = observeEvent('polkadotVault:AwaitingGovernanceActivation').event;
  const btcActivationRequest = observeEvent('bitcoinVault:AwaitingGovernanceActivation').event;
  const arbActivationRequest = observeEvent('arbitrumVault:AwaitingGovernanceActivation').event;
  const solActivationRequest = observeEvent('solanaVault:AwaitingGovernanceActivation').event;
  const hubActivationRequest = observeEvent('assethubVault:AwaitingGovernanceActivation').event;
  const dotKey = (await dotActivationRequest).data.newPublicKey;
  const btcKey = (await btcActivationRequest).data.newPublicKey;
  const arbKey = (await arbActivationRequest).data.newPublicKey;
  const solKey = (await solActivationRequest).data.newPublicKey;
  const hubKey = (await hubActivationRequest).data.newPublicKey;

  // Step 4
  console.log('Requesting Polkadot Vault creation');
  const { vaultAddress: dotVaultAddress, vaultExtrinsicIndex: dotVaultExtrinsicIndex } = await createPolkadotVault(polkadot);
  const dotProxyAdded = observeEvent('proxy:ProxyAdded', { chain: 'polkadot' }).event;

  console.log('Requesting Assethub Vault creation');
  const { vaultAddress: hubVaultAddress, vaultExtrinsicIndex: hubVaultExtrinsicIndex } = await createPolkadotVault(assethub);
  const hubProxyAdded = observeEvent('proxy:ProxyAdded', { chain: 'assethub' }).event;

  // Step 5
  console.log('Rotating Proxy and Funding Accounts on Polkadot and Assethub');
  const [, , dotVaultEvent, hubVaultEvent] = await Promise.all([
    rotateAndFund(polkadot, dotVaultAddress, dotKey),
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    dotProxyAdded,
    hubProxyAdded,
  ]);

  // Step 6
  console.log('Inserting Arbitrum key in the contracts');
  await initializeArbitrumContracts(arbClient, arbKey);

  // Using arbitrary key for now, we will use solKey generated by SC
  console.log('Inserting Solana key in the programs');
  await initializeSolanaPrograms(solClient, solKey);

  // Step 7
  console.log('Registering Vaults with state chain');
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
  console.log('Creating USDC and USDT tokens on Assethub');
  await assethub.tx.assets.create(1337, alice.address, 10000).signAndSend(alice, {nonce: -1});
  await assethub.tx.assets.create(1984, alice.address, 10000).signAndSend(alice, {nonce: -1});
  await assethub.tx.assets.setMetadata(1337, "USD Coin", "USDC", 6).signAndSend(alice, {nonce: -1});
  await assethub.tx.assets.setMetadata(1984, "Tether USD", "USDT", 6).signAndSend(alice, {nonce: -1});
  await assethub.tx.assets.mint(1337, alice.address, 100000000000000).signAndSend(alice, {nonce: -1});
  await assethub.tx.assets.mint(1984, alice.address, 100000000000000).signAndSend(alice, {nonce: -1});

  // Confirmation
  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch').event;

  console.log('=== New Epoch ===');
  console.log('=== Vault Setup completed ===');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
