// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: pnpm tsx ./commands/setup_vaults.ts

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { getChainflipApi, sleep } from '../shared/utils';

async function main(): Promise<void> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);
  const alice_uri = process.env.POLKADOT_ALICE_URI || "//Alice";
  const alice = keyring.createFromUri(alice_uri);

  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Forcing rotation');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.validator.forceRotation())
    .signAndSend(snowwhite);

  // Step 2
  console.log('Waiting for new keys');
  let btcKey: string | undefined;
  let waitingForBtcKey = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === 'bitcoinVault' && event.method === 'AwaitingGovernanceActivation') {
        btcKey = event.data[0];
        unsubscribe();
        console.log('Found BTC AggKey');
        waitingForBtcKey = false;
      }
    });
  });
  while (waitingForBtcKey) {
    await sleep(1000);
  }

  // Step 8
  // Fake the dot aggkey and vault.
  console.log('Registering Vaults with state chain');
  const txid = { blockNumber: 1, extrinsicIndex: 1 };
  const dotWitnessing = chainflip.tx.environment.witnessPolkadotVaultCreation(
    "cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf",
    "cfcfcfcfcfcfcfcfcfcfcfcfcfcfcfcf",
    txid,
    1,
  );
  const myDotTx = chainflip.tx.governance.proposeGovernanceExtrinsic(dotWitnessing);
  let done = false;
  unsubscribe = await myDotTx.signAndSend(snowwhite, { nonce: -1 }, (result) => {
    if (result.status.isInBlock) {
      console.log(`Dot vault registered at blockHash ${result.status.asInBlock}`);
      unsubscribe();
      done = true;
    }
  });
  while (!done) {
    await sleep(1000);
  }

  const btcWitnessing = chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(1, btcKey);
  const myBtcTx = chainflip.tx.governance.proposeGovernanceExtrinsic(btcWitnessing);
  await myBtcTx.signAndSend(snowwhite, { nonce: -1 });

  // Confirmation
  console.log('Waiting for new epoch');
  let waitingForEvent = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  unsubscribe = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === 'validator' && event.method === 'NewEpoch') {
        unsubscribe();
        waitingForEvent = false;
      }
    });
  });
  while (waitingForEvent) {
    await sleep(1000);
  }
  console.log('=== Vault Setup completed ===');
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
