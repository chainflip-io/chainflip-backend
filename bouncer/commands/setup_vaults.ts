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
  getChainflipApi,
  getPolkadotApi,
  getBtcClient,
  observeEvent,
  sleep,
  handleSubstrateError,
  getEvmEndpoint,
  getEvmContractAddress,
} from '../shared/utils';
import { aliceKeyringPair } from '../shared/polkadot_keyring';
import { getKeyManagerAbi } from '../shared/eth_abis';
import { signAndSendTxEvm } from '../shared/send_evm';

async function main(): Promise<void> {
  const btcClient = getBtcClient();
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));
  const alice = await aliceKeyringPair();

  const chainflip = await getChainflipApi();
  const polkadot = await getPolkadotApi();

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Initializing Arbitrum');
  const arbInitializationRequest = observeEvent('arbitrumVault:ChainInitialized', chainflip);
  await submitGovernanceExtrinsic(chainflip.tx.arbitrumVault.initializeChain());
  await arbInitializationRequest;

  // Step 2
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());

  // Step 3
  console.log('Waiting for new keys');

  const dotActivationRequest = observeEvent(
    'polkadotVault:AwaitingGovernanceActivation',
    chainflip,
  );
  const btcActivationRequest = observeEvent('bitcoinVault:AwaitingGovernanceActivation', chainflip);

  const arbActivationRequest = observeEvent(
    'arbitrumVault:AwaitingGovernanceActivation',
    chainflip,
  );
  const dotKey = (await dotActivationRequest).data.newPublicKey;
  const btcKey = (await btcActivationRequest).data.newPublicKey;
  const arbKey = (await arbActivationRequest).data.newPublicKey;

  // Step 4
  console.log('Requesting Polkadot Vault creation');
  const createPolkadotVault = async () => {
    let vaultAddress: AddressOrPair | undefined;
    let vaultExtrinsicIndex: number | undefined;
    const unsubscribe = await polkadot.tx.proxy
      .createPure(polkadot.createType('ProxyType', 'Any'), 0, 0)
      .signAndSend(alice, { nonce: -1 }, (result) => {
        if (result.isError) {
          handleSubstrateError(result);
        }
        if (result.isInBlock) {
          console.log('Polkadot Vault created');
          // TODO: figure out type inference so we don't have to coerce using `any`
          const pureCreated = result.findRecord('proxy', 'PureCreated')!;
          vaultAddress = pureCreated.event.data[0] as AddressOrPair;
          vaultExtrinsicIndex = result.txIndex!;
          unsubscribe();
        }
      });
    while (vaultAddress === undefined) {
      await sleep(3000);
    }
    return { vaultAddress, vaultExtrinsicIndex };
  };
  const { vaultAddress, vaultExtrinsicIndex } = await createPolkadotVault();

  const proxyAdded = observeEvent('proxy:ProxyAdded', polkadot);

  // Step 5
  console.log('Rotating Proxy and Funding Accounts.');
  const rotateAndFund = async () => {
    let done = false;
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
        polkadot.tx.balances.transfer(vaultAddress, 1000000000000),
        polkadot.tx.balances.transfer(dotKey, 1000000000000),
        rotation,
      ])
      .signAndSend(alice, { nonce: -1 }, (result) => {
        if (result.isError) {
          handleSubstrateError(result);
        }
        if (result.isInBlock) {
          unsubscribe();
          done = true;
        }
      });
    while (!done) {
      await sleep(3000);
    }
  };
  await rotateAndFund();
  const vaultBlockNumber = (await proxyAdded).block;

  // Step 6
  console.log('Inserting Arbitrum key in the contracts');
  const keyManagerAddress = getEvmContractAddress('Arbitrum', 'KEY_MANAGER');
  const web3 = new Web3(getEvmEndpoint('Arbitrum'));

  const keyManagerContract = new web3.eth.Contract(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await getKeyManagerAbi()) as any,
    keyManagerAddress,
  );
  const txData = keyManagerContract.methods
    .setAggKeyWithGovKey({
      pubKeyX: arbKey.pubKeyX,
      pubKeyYParity: arbKey.pubKeyYParity === 'Odd' ? 1 : 0,
    })
    .encodeABI();
  await signAndSendTxEvm('Arbitrum', keyManagerAddress, '0', txData);

  // Step 7
  console.log('Registering Vaults with state chain');
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessPolkadotVaultCreation(vaultAddress, {
      blockNumber: vaultBlockNumber,
      extrinsicIndex: vaultExtrinsicIndex,
    }),
  );
  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
      await btcClient.getBlockCount(),
      btcKey,
    ),
  );

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );

  // Confirmation
  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch', chainflip);

  console.log('=== New Epoch ===');
  console.log('=== Vault Setup completed ===');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
