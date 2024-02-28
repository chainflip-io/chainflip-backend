#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_vaults.ts

import { AddressOrPair } from '@polkadot/api/types';
import {
  NonceAccount,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from '@solana/web3.js';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import {
  getChainflipApi,
  getPolkadotApi,
  getBtcClient,
  observeEvent,
  sleep,
  handleSubstrateError,
  getSolConnection,
  getContractAddress,
  getSolWhaleKeyPair,
  encodeSolAddress,
  getEncodedSolAddress,
} from '../shared/utils';
import { aliceKeyringPair } from '../shared/polkadot_keyring';
import { signAndSendTxSol } from '../shared/send_sol';
import { getSolanaVaultIdl } from '../shared/eth_abis';

async function main(): Promise<void> {
  const btcClient = getBtcClient();
  // const solClient = getSolConnection();
  const alice = await aliceKeyringPair();

  const chainflip = await getChainflipApi();
  const polkadot = await getPolkadotApi();

  console.log('=== Performing initial Vault setup ===');

  // Step 1
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());

  // Step 2
  console.log('Waiting for new keys');

  const dotActivationRequest = observeEvent(
    'polkadotVault:AwaitingGovernanceActivation',
    chainflip,
  );
  const btcActivationRequest = observeEvent('bitcoinVault:AwaitingGovernanceActivation', chainflip);
  // const solActivationRequest = observeEvent('solanaVault:AwaitingGovernanceActivation', chainflip);
  const dotKey = (await dotActivationRequest).data.newPublicKey;
  const btcKey = (await btcActivationRequest).data.newPublicKey;
  // const solKey = (await solActivationRequest).data.newPublicKey;

  // Step 3
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

  // // Step 5
  // console.log('Inserting keys in the Solana program');
  // const solKey = '0x25fcb03ab6435d106b5df1e677f3c6a10a7b22719deedeb3761c005e1306423d';

  // const solanaVaultProgramId = new PublicKey(getContractAddress('Solana', 'VAULT'));
  // const dataAccount = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));
  // const whaleKeypair = getSolWhaleKeyPair();
  // const vaultIdl = await getSolanaVaultIdl();

  // const discriminatorString = vaultIdl.instructions.find(
  //   (instruction: { name: string }) => instruction.name === 'initialize',
  // ).discriminator;
  // const discriminator = new Uint8Array(JSON.parse(discriminatorString));

  // const solKeyBuffer = Buffer.from(solKey.slice(2), 'hex');

  // // Convert PublicKey to Buffer
  // const dataBuffer = Buffer.concat([Buffer.from(discriminator.buffer), solKeyBuffer]);
  // console.log('dataBuffer', dataBuffer);

  // const tx = new Transaction().add(
  //   new TransactionInstruction({
  //     data: dataBuffer,
  //     keys: [
  //       { pubkey: dataAccount, isSigner: false, isWritable: true },
  //       { pubkey: whaleKeypair.publicKey, isSigner: true, isWritable: false },
  //       { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  //     ],
  //     programId: solanaVaultProgramId,
  //   }),
  // );

  // await signAndSendTxSol(tx);

  // // For now just deriving the nonceAccounts from the whaleKeypair with index seeds
  // const numberNonceAccounts = 5;
  // const nonceAccounts = [];
  // const nonceValues = [];
  // for (let i = 0; i < numberNonceAccounts; i++) {
  //   // Using the index stringified as the seed ('0', '1', '2' ...)
  //   const seed = i.toString();
  //   const nonceAccount = await PublicKey.createWithSeed(
  //     whaleKeypair.publicKey,
  //     seed,
  //     SystemProgram.programId,
  //   );
  //   nonceAccounts.push(nonceAccount);

  //   const nonceAccountInfo = await solClient.getAccountInfo(new PublicKey(nonceAccount));
  //   const nonceValue = NonceAccount.fromAccountData(nonceAccountInfo!.data).nonce;
  //   nonceValues.push(nonceValue);
  // }

  // const nonceAuthorizeTransaction = new Transaction();
  // for (const nonceAccount of nonceAccounts) {
  //   nonceAuthorizeTransaction.add(
  //     SystemProgram.nonceAuthorize({
  //       noncePubkey: new PublicKey(nonceAccount),
  //       authorizedPubkey: whaleKeypair.publicKey,
  //       newAuthorizedPubkey: new PublicKey(encodeSolAddress(solKey)),
  //     }),
  //   );
  // }
  // await signAndSendTxSol(nonceAuthorizeTransaction);

  // Step 6
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

  // TODO: This call should contain the new vault address,
  // nonce accounts and probably nonce values
  // await submitGovernanceExtrinsic(
  //   chainflip.tx.environment.witnessInitializeSolanaVault(
  //   ),
  // );

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
