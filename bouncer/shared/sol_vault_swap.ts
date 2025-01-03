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
import { getAssociatedTokenAddressSync } from '@solana/spl-token';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  getSolWhaleKeyPair,
  getSolConnection,
  sleep,
  createStateChainKeypair,
  stateChainAssetFromAsset,
  decodeSolAddress,
  decodeDotAddressForContract,
} from './utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from './new_swap';

import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/swap_endpoint';
import { getSolanaSwapEndpointIdl } from './contract_interfaces';
import { getChainflipApi } from './utils/substrate';

const createdEventAccounts: PublicKey[] = [];

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

// TODO: Unify this with BTC & EVM vault swaps.
// interface BitcoinVaultSwapExtraParameters {
//   chain: 'Bitcoin';
//   min_output_amount: number;
//   retry_duration: number;
// }
// type VaultSwapExtraParameters = BitcoinVaultSwapExtraParameters | SolanaVaultSwapExtraParameters;

type ChannelRefundParameters = {
  retry_duration: number;
  refund_address: string;
  min_price: string;
};

export async function executeSolVaultSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
) {
  const whaleKeypair = getSolWhaleKeyPair();

  const connection = getSolConnection();

  const newEventAccountKeypair = Keypair.generate();
  createdEventAccounts.push(newEventAccountKeypair.publicKey);

  const amountToSwap = amountToFineAmount(
    amount ?? defaultAssetAmounts(srcAsset),
    assetDecimals(srcAsset),
  );

  await using chainflip = await getChainflipApi();
  const brokerUri = '//BROKER_1';
  const broker = createStateChainKeypair(brokerUri);

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
    input_amount: amountToSwap,
    refund_parameters: refundParams,
    from_token_account:
      srcAsset === 'Sol'
        ? undefined
        : getAssociatedTokenAddressSync(
            new PublicKey(getContractAddress('Solana', 'SolUsdc')),
            whaleKeypair.publicKey,
            false,
          ).toString(),
  };

  const vaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    { chain: chainFromAsset(srcAsset), asset: stateChainAssetFromAsset(srcAsset) },
    { chain: chainFromAsset(destAsset), asset: stateChainAssetFromAsset(destAsset) },
    chainFromAsset(destAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destAddress)
      : destAddress,
    0, // broker_commission
    extraParameters, // extra_parameters
    // channel_metadata
    messageMetadata && {
      message: messageMetadata.message as `0x${string}`,
      gas_budget: messageMetadata.gasBudget,
      ccm_additional_data: messageMetadata.ccmAdditionalData,
    },
    boostFeeBps ?? 0, // boost_fee
    null, // affiliates
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

  const txHash = await sendAndConfirmTransaction(
    connection,
    transaction,
    [whaleKeypair, newEventAccountKeypair],
    { commitment: 'confirmed' },
  );

  const transactionData = await connection.getTransaction(txHash, { commitment: 'confirmed' });
  if (transactionData === null) {
    throw new Error('Solana TransactionData is empty');
  }
  return { txHash, slot: transactionData!.slot, accountAddress: newEventAccountKeypair.publicKey };
}

export async function checkSolEventAccountsClosure(
  eventAccounts: PublicKey[] = createdEventAccounts,
) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const SwapEndpointIdl: any = await getSolanaSwapEndpointIdl();
  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(
    SwapEndpointIdl as SwapEndpoint,
    { connection: getSolConnection() } as anchor.Provider,
  );
  const swapEndpointDataAccountAddress = new PublicKey(
    getContractAddress('Solana', 'SWAP_ENDPOINT_DATA_ACCOUNT'),
  );

  const maxRetries = 50; // 300 seconds

  for (let attempt = 0; attempt < maxRetries; attempt++) {
    const swapEndpointDataAccount =
      await cfSwapEndpointProgram.account.swapEndpointDataAccount.fetch(
        swapEndpointDataAccountAddress,
      );

    if (swapEndpointDataAccount.openEventAccounts.length >= 10) {
      await sleep(6000);
    } else {
      const onChainOpenedAccounts = swapEndpointDataAccount.openEventAccounts.map((element) =>
        element.toString(),
      );
      for (const eventAccount of eventAccounts) {
        if (!onChainOpenedAccounts.includes(eventAccount.toString())) {
          const accountInfo = await getSolConnection().getAccountInfo(eventAccount);
          if (accountInfo !== null) {
            throw new Error('Event account still exists, should have been closed');
          }
        }
      }
      return;
    }
  }
  throw new Error('Timed out waiting for event accounts to be closed');
}
