// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: pnpm tsx ./commands/setup_vaults.ts

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { getChainflipApi, getPolkadotApi, sleep, handleSubstrateError } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function main(): Promise<void> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const alice_uri = process.env.POLKADOT_ALICE_URI || "//Alice";
  const alice = keyring.createFromUri(alice_uri);

  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  const polkadot = await getPolkadotApi(process.env.POLKADOT_ENDPOINT);

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());

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
  await polkadot.tx.balances.transfer(dotKeyAddress, 1000000000000).signAndSend(alice, {nonce: -1}, handleSubstrateError(polkadot));

  // Step 4
  console.log('Requesting Polkadot Vault creation');
  await submitGovernanceExtrinsic(chainflip.tx.environment.createPolkadotVault(dotKey));

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
  await polkadot.tx.balances.transfer(vaultAddress, 1000000000000).signAndSend(alice, {nonce: -1}, handleSubstrateError(polkadot));

  // Step 8
  console.log('Registering Vaults with state chain');
  const txid = { blockNumber: vaultBlock, extrinsicIndex: vaultEventIndex };

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessPolkadotVaultCreation(
      vaultAddress,
      dotKey,
      txid,
      1
    )
  );

  await submitGovernanceExtrinsic(chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(1, btcKey));

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
