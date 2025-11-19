import assert from 'assert';
import * as anchor from '@coral-xyz/anchor';
import { InternalAsset as Asset, broker } from '@chainflip/cli';
import { brokerApiEndpoint } from 'shared/json_rpc';
import {
  PublicKey,
  sendAndConfirmTransaction,
  TransactionInstruction,
  Transaction,
} from '@solana/web3.js';
import { randomBytes } from 'crypto';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  assetDecimals,
  getSolWhaleKeyPair,
  getSolConnection,
  sleep,
  stateChainAssetFromAsset,
  observeFetch,
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';

import { getSolanaSwapEndpointIdl } from 'shared/contract_interfaces';
import { getBalance } from 'shared/get_balance';
import { TestContext } from 'shared/utils/test_context';
import { Logger, throwError } from 'shared/utils/logger';
import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.2.3/swap_endpoint';

const createdEventAccounts: [PublicKey, boolean][] = [];

export async function executeSolVaultSwap(
  logger: Logger,
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerFees: {
    account: string;
    commissionBps: number;
  },
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  affiliateFees: {
    account: string;
    bps: number;
  }[] = [],
) {
  const whaleKeypair = getSolWhaleKeyPair();

  const connection = getSolConnection();

  const seed = randomBytes(32);
  const [newEventAccountPublicKey] = PublicKey.findProgramAddressSync(
    [Buffer.from('swap_event'), whaleKeypair.publicKey.toBuffer(), seed],
    new PublicKey(getContractAddress('Solana', 'SWAP_ENDPOINT')),
  );
  createdEventAccounts.push([newEventAccountPublicKey, srcAsset === 'Sol']);

  const amountToSwap = amountToFineAmount(
    amount ?? defaultAssetAmounts(srcAsset),
    assetDecimals(srcAsset),
  );

  const vaultSwapDetails = await broker.requestSwapParameterEncoding(
    {
      srcAsset: stateChainAssetFromAsset(srcAsset),
      srcAddress: whaleKeypair.publicKey.toBase58(),
      destAsset: stateChainAssetFromAsset(destAsset),
      destAddress,
      commissionBps: brokerFees.commissionBps,
      ccmParams: messageMetadata && {
        message: messageMetadata.message,
        gasBudget: messageMetadata.gasBudget,
        ccmAdditionalData: messageMetadata.ccmAdditionalData,
      },
      fillOrKillParams: fillOrKillParams ?? {
        retryDurationBlocks: 0,
        refundAddress: whaleKeypair.publicKey.toBase58(),
        minPriceX128: '0',
      },
      maxBoostFeeBps: boostFeeBps ?? 0,
      amount: amountToSwap,
      dcaParams: dcaParams && {
        numberOfChunks: dcaParams.numberOfChunks,
        chunkIntervalBlocks: dcaParams.chunkIntervalBlocks,
      },
      extraParams: {
        seed: `0x${seed.toString('hex')}`,
      },
      affiliates: affiliateFees.map((fee) => ({
        account: fee.account,
        commissionBps: fee.bps,
      })),
    },
    {
      url: brokerApiEndpoint,
    },
    'backspin',
  );

  logger.trace('Requesting vault swap parameter encoding');

  assert.strictEqual(vaultSwapDetails.chain, 'Solana');
  assert.strictEqual(
    new PublicKey(vaultSwapDetails.programId).toBase58(),
    getContractAddress('Solana', 'SWAP_ENDPOINT'),
  );

  const transaction = new Transaction();
  const instruction = new TransactionInstruction({
    keys: vaultSwapDetails.accounts.map((account) => ({
      pubkey: new PublicKey(account.pubkey),
      isSigner: account.isSigner,
      isWritable: account.isWritable,
    })),
    programId: new PublicKey(vaultSwapDetails.programId),
    data: Buffer.from(vaultSwapDetails.data.slice(2), 'hex'),
  });

  transaction.add(instruction);

  logger.trace('Sending Solana vault swap transaction');
  const txHash = await sendAndConfirmTransaction(connection, transaction, [whaleKeypair], {
    commitment: 'confirmed',
  });

  const transactionData = await connection.getTransaction(txHash, {
    commitment: 'confirmed',
    maxSupportedTransactionVersion: 0,
  });
  if (transactionData === null) {
    throwError(logger, new Error('Solana TransactionData is empty'));
  }
  return { txHash, slot: transactionData!.slot, accountAddress: newEventAccountPublicKey };
}

const MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES = 5;
export async function checkSolEventAccountsClosure(
  testContext: TestContext,
  eventAccounts: [PublicKey, boolean][] = createdEventAccounts,
) {
  testContext.info('Checking Solana Vault Swap Account Closure');

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const SwapEndpointIdl: any = await getSolanaSwapEndpointIdl();
  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(
    SwapEndpointIdl as SwapEndpoint,
    { connection: getSolConnection() } as anchor.Provider,
  );
  const swapEndpointDataAccountAddress = new PublicKey(
    getContractAddress('Solana', 'SWAP_ENDPOINT_DATA_ACCOUNT'),
  );

  const maxRetries = 20; // 120 seconds
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    const swapEndpointDataAccount =
      await cfSwapEndpointProgram.account.swapEndpointDataAccount.fetch(
        swapEndpointDataAccountAddress,
      );
    const onChainOpenedAccounts = swapEndpointDataAccount.openEventAccounts.map((element) =>
      element.toString(),
    );

    // All native SOL must have been closed. SPL-token might or might not be closed. However,
    // no more than MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES can be opened at the end.
    const nativeEventAccounts: PublicKey[] = eventAccounts
      .filter((eventAccount) => eventAccount[1])
      .map((eventAccount) => eventAccount[0]);

    if (
      onChainOpenedAccounts.length > MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES ||
      nativeEventAccounts.some((eventAccount) =>
        onChainOpenedAccounts.includes(eventAccount.toString()),
      )
    ) {
      // Some account is not closed yet
      await sleep(6000);
    } else {
      for (const nativeEventAccount of nativeEventAccounts) {
        // Ensure native accounts are closed correctly
        const accountInfo = await getSolConnection().getAccountInfo(nativeEventAccount);
        const balanceEventAccount = Number(await getBalance('Sol', nativeEventAccount.toString()));
        if (accountInfo !== null || balanceEventAccount > 0) {
          throw new Error(
            'This should never happen, a closed account should have no data nor balance',
          );
        }
      }
      // Swap Endpoint's native vault should always have been fetched.
      await observeFetch('Sol', getContractAddress('Solana', 'SWAP_ENDPOINT_NATIVE_VAULT_ACCOUNT'));
      return;
    }
  }
  throw new Error('Timed out waiting for event accounts to be closed');
}
