import assert from 'assert';
import * as anchor from '@coral-xyz/anchor';
import { InternalAsset as Asset, Chains } from '@chainflip/cli';
import {
  PublicKey,
  Keypair,
  sendAndConfirmTransaction,
  TransactionInstruction,
  Transaction,
  AccountMeta,
} from '@solana/web3.js';
import BigNumber from 'bignumber.js';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  getSolWhaleKeyPair,
  getSolConnection,
  sleep,
  stateChainAssetFromAsset,
  decodeSolAddress,
  decodeDotAddressForContract,
  observeFetch,
} from './utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from './new_swap';

import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.1-swap-endpoint/swap_endpoint';
import { getSolanaSwapEndpointIdl } from './contract_interfaces';
import { getChainflipApi } from './utils/substrate';
import { getBalance } from './get_balance';
import { TestContext } from './utils/test_context';
import { Logger, throwError } from './utils/logger';

const createdEventAccounts: [PublicKey, boolean][] = [];

interface SolVaultSwapDetails {
  chain: string;
  program_id: string;
  accounts: RpcAccountMeta[];
  data: string;
}

type RpcAccountMeta = {
  pubkey: string;
  is_signer: boolean;
  is_writable: boolean;
};

interface SolanaVaultSwapExtraParameters {
  chain: 'Solana';
  from: string;
  event_data_account: string;
  input_amount: string;
  refund_parameters: ChannelRefundParameters;
  from_token_account?: string;
}

export type ChannelRefundParameters = {
  retry_duration: number;
  refund_address: string;
  min_price: string;
};

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

  const newEventAccountKeypair = Keypair.generate();
  createdEventAccounts.push([newEventAccountKeypair.publicKey, srcAsset === 'Sol']);

  const amountToSwap = amountToFineAmount(
    amount ?? defaultAssetAmounts(srcAsset),
    assetDecimals(srcAsset),
  );

  await using chainflip = await getChainflipApi();

  const refundParams: ChannelRefundParameters = {
    retry_duration: fillOrKillParams?.retryDurationBlocks ?? 0,
    refund_address: decodeSolAddress(
      fillOrKillParams?.refundAddress ?? whaleKeypair.publicKey.toBase58(),
    ),
    min_price: fillOrKillParams?.minPriceX128 ?? '0x0',
  };

  const extraParameters: SolanaVaultSwapExtraParameters = {
    chain: 'Solana',
    from: decodeSolAddress(whaleKeypair.publicKey.toBase58()),
    event_data_account: decodeSolAddress(newEventAccountKeypair.publicKey.toBase58()),
    input_amount: '0x' + new BigNumber(amountToSwap).toString(16),
    refund_parameters: refundParams,
    from_token_account: undefined,
  };

  logger.trace('Requesting vault swap parameter encoding');
  const vaultSwapDetails = (await chainflip.rpc(
    `cf_request_swap_parameter_encoding`,
    brokerFees.account,
    { chain: chainFromAsset(srcAsset), asset: stateChainAssetFromAsset(srcAsset) },
    { chain: chainFromAsset(destAsset), asset: stateChainAssetFromAsset(destAsset) },
    chainFromAsset(destAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destAddress)
      : destAddress,
    brokerFees.commissionBps,
    extraParameters,
    messageMetadata && {
      message: messageMetadata.message as `0x${string}`,
      gas_budget: messageMetadata.gasBudget,
      ccm_additional_data: messageMetadata.ccmAdditionalData as `0x${string}`,
    },
    boostFeeBps ?? 0,
    affiliateFees,
    dcaParams && {
      number_of_chunks: dcaParams.numberOfChunks,
      chunk_interval: dcaParams.chunkIntervalBlocks,
    },
  )) as unknown as SolVaultSwapDetails;

  assert.strictEqual(vaultSwapDetails.chain, 'Solana');
  assert.strictEqual(
    new PublicKey(vaultSwapDetails.program_id).toBase58(),
    getContractAddress('Solana', 'SWAP_ENDPOINT'),
  );

  // Convert vaultSwapDetails.instruction.accounts into AccountMeta[]
  const keys: AccountMeta[] = [];
  for (const account of vaultSwapDetails.accounts) {
    keys.push({
      pubkey: new PublicKey(account.pubkey),
      isSigner: account.is_signer,
      isWritable: account.is_writable,
    });
  }

  const transaction = new Transaction();
  const instruction = new TransactionInstruction({
    keys,
    programId: new PublicKey(vaultSwapDetails.program_id),
    data: Buffer.from(vaultSwapDetails.data.slice(2), 'hex'),
  });

  transaction.add(instruction);

  logger.trace('Sending Solana vault swap transaction');
  const txHash = await sendAndConfirmTransaction(
    connection,
    transaction,
    [whaleKeypair, newEventAccountKeypair],
    { commitment: 'confirmed' },
  );

  const transactionData = await connection.getTransaction(txHash, { commitment: 'confirmed' });
  if (transactionData === null) {
    throwError(logger, new Error('Solana TransactionData is empty'));
  }
  return { txHash, slot: transactionData!.slot, accountAddress: newEventAccountKeypair.publicKey };
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
