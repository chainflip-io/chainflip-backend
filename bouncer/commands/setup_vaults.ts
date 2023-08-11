#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { AddressOrPair } from '@polkadot/api/types';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import {
  getChainflipApi,
  getPolkadotApi,
  getBtcClient,
  observeEvent,
  sleep,
  handleSubstrateError,
} from '../shared/utils';

async function main(): Promise<void> {
  const btcClient = getBtcClient();
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const aliceUri = process.env.POLKADOT_ALICE_URI || '//Alice';
  const alice = keyring.createFromUri(aliceUri);

  const chainflip = await getChainflipApi();
  const polkadot = await getPolkadotApi();

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());

  // Step 2
  console.log('Waiting for new keys');

  const btcActivationRequest = observeEvent(
    'polkadotVault:AwaitingGovernanceActivation',
    chainflip,
  );
  const dotActivationRequest = observeEvent('bitcoinVault:AwaitingGovernanceActivation', chainflip);
  const dotKey = (await btcActivationRequest).data.newPublicKey;
  const btcKey = (await dotActivationRequest).data.newPublicKey;

  // Step 3
  console.log('Requesting Polkadot Vault creation');
  const createPolkadotVault = async () => {
    let vaultAddress: AddressOrPair | undefined;
    let vaultExtrinsicIndex: number | undefined;
    let vaultBlockHash: Uint8Array | undefined;
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
          vaultBlockHash = result.dispatchInfo!.createdAtHash!;
          unsubscribe();
        }
      });
    const vaultBlockNumber = (await polkadot.rpc.chain.getHeader(vaultBlockHash)).number.toNumber();
    while (vaultAddress === undefined) {
      await sleep(3000);
    }
    return { vaultAddress, vaultExtrinsicIndex, vaultBlockNumber };
  };
  const { vaultAddress, vaultExtrinsicIndex, vaultBlockNumber } = await createPolkadotVault();

  // Step 4
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
          console.log(
            `Proxy rotated and accounts funded at block `,
            result.toHuman().status.InBlock,
          );
          unsubscribe();
          done = true;
        }
      });
    while (!done) {
      await sleep(3000);
    }
  };
  await rotateAndFund();

  // Step 5
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
