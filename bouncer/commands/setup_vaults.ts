#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.sh

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { sleep } from '../shared/utils';

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const polkadotEndpoint = process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9945';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);
  const alice_uri = process.env.POLKADOT_ALICE_URI || "//Alice";
  const alice = keyring.createFromUri(alice_uri);
  const chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
  const polkadot = await ApiPromise.create({
    provider: new WsProvider(polkadotEndpoint),
    noInitWarn: true,
  });

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Forcing rotation');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.validator.forceRotation())
    .signAndSend(snowwhite);

  // Step 2
  console.log('Waiting for new keys');
  let dotKey: string | undefined;
  let btcKey: string | undefined;
  let waitingForDotKey = true;
  let waitingForBtcKey = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === 'polkadotVault' && event.method === 'AwaitingGovernanceActivation') {
        dotKey = event.data[0];
        if (!waitingForBtcKey) {
          unsubscribe();
        }
        console.log('Found DOT AggKey');
        waitingForDotKey = false;
      }
      if (event.section === 'bitcoinVault' && event.method === 'AwaitingGovernanceActivation') {
        btcKey = event.data[0];
        if (!waitingForDotKey) {
          unsubscribe();
        }
        console.log('Found BTC AggKey');
        waitingForBtcKey = false;
      }
    });
  });
  while (waitingForBtcKey || waitingForDotKey) {
    await sleep(1000);
  }
  const dotKeyAddress = keyring.encodeAddress(dotKey as string, 0);

  // Step 3
  console.log('Transferring 100 DOT to Polkadot AggKey');
  await polkadot.tx.balances.transfer(dotKeyAddress, 1000000000000).signAndSend(alice);

  // Step 4
  console.log('Requesting Polkadot Vault creation');
  const createCommand = chainflip.tx.environment.createPolkadotVault(dotKey);
  const mytx = chainflip.tx.governance.proposeGovernanceExtrinsic(createCommand);
  await mytx.signAndSend(snowwhite);

  // Step 5
  console.log('Waiting for Vault address on Polkadot chain');
  let vaultAddress: string | undefined;
  let vaultBlock: number | undefined;
  let vaultEventIndex: number | undefined;
  let waitingForEvent = true;
  unsubscribe = await polkadot.rpc.chain.subscribeNewHeads(async (header) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const events: any[] = await polkadot.query.system.events.at(header.hash);
    events.forEach((record, index) => {
      const { event } = record;
      if (event.section === 'proxy' && event.method === 'PureCreated') {
        vaultAddress = event.data[0];
        vaultBlock = header.number.toNumber();
        vaultEventIndex = index;
        unsubscribe();
        waitingForEvent = false;
      }
    });
  });
  while (waitingForEvent) {
    await sleep(1000);
  }
  console.log('Found DOT Vault with address ' + (vaultAddress as string));

  // Step 7
  console.log('Transferring 100 DOT to Polkadot Vault');
  await polkadot.tx.balances.transfer(vaultAddress, 1000000000000).signAndSend(alice);

  // Step 8
  console.log('Registering Vaults with state chain');
  const txid = { blockNumber: vaultBlock, extrinsicIndex: vaultEventIndex };
  const dotWitnessing = chainflip.tx.environment.witnessPolkadotVaultCreation(
    vaultAddress,
    dotKey,
    txid,
    1,
  );
  const myDotTx = chainflip.tx.governance.proposeGovernanceExtrinsic(dotWitnessing);
  await myDotTx.signAndSend(snowwhite, { nonce: -1 });

  const btcWitnessing = chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(1, btcKey);
  const myBtcTx = chainflip.tx.governance.proposeGovernanceExtrinsic(btcWitnessing);
  await myBtcTx.signAndSend(snowwhite, { nonce: -1 });

  // Confirmation
  console.log('Waiting for new epoch');
  waitingForEvent = true;
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
