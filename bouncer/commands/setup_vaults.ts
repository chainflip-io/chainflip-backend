#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { send } from '../shared/send';
import { getChainflipApi, getPolkadotApi, getBtcClient, observeEvent, decodeDotAddressForContract } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function main(): Promise<void> {
  const btcClient = getBtcClient();

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
  console.log('Transferring 100 DOT to Polkadot AggKey');
  await send('DOT', dotKey, '100');

  // Step 4
  console.log('Requesting Polkadot Vault creation');
  await submitGovernanceExtrinsic(chainflip.tx.environment.createPolkadotVault(dotKey));

  // Step 5
  console.log('Waiting for Vault address on Polkadot chain');
  const vaultEvent = await observeEvent('proxy:PureCreated', polkadot);
  const vaultAddress = vaultEvent.data.pure as string;
  const vaultBlock = vaultEvent.block;
  const vaultEventIndex = vaultEvent.event_index;

  console.log('Found DOT Vault with address ' + vaultAddress);

  // Step 7
  console.log('Transferring 100 DOT to Polkadot Vault');
  await send('DOT', vaultAddress, '100');

  // Step 8
  console.log('Registering Vaults with state chain');
  const txid = { blockNumber: vaultBlock, extrinsicIndex: vaultEventIndex };

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessPolkadotVaultCreation(decodeDotAddressForContract(vaultAddress), dotKey, txid, 1),
  );

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(
      await btcClient.getBlockCount(),
      btcKey,
    ),
  );

  // Confirmation
  console.log('Waiting for new epoch');
  await observeEvent('validator:NewEpoch', chainflip);
  console.log('=== Vault Setup completed ===');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
