import assert from 'assert';
import * as anchor from '@coral-xyz/anchor';
import { InternalAsset as Asset, Chains, assetConstants } from '@chainflip/cli';
import {
  PublicKey,
  Keypair,
  sendAndConfirmTransaction,
  TransactionInstruction,
  Transaction,
  AccountMeta,
} from '@solana/web3.js';
import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from '@solana/spl-token';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  getSolWhaleKeyPair,
  getSolConnection,
  chainContractId,
  decodeDotAddressForContract,
  sleep,
  createStateChainKeypair,
  stateChainAssetFromAsset,
  decodeSolAddress,
} from './utils';
import { CcmDepositMetadata } from './new_swap';

import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/swap_endpoint';
import { Vault } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/vault';
import { getSolanaSwapEndpointIdl, getSolanaVaultIdl } from './contract_interfaces';
import { getChainflipApi } from './utils/substrate';

// @ts-expect-error workaround because of anchor issue
const { BN } = anchor.default;

// Using AnchorProvider runs into issues so instead we store the wallet in id.json and then
// set the ANCHOR_WALLET env. Depending on how the SDK is implemented we can remove this.
process.env.ANCHOR_WALLET = 'shared/solana_keypair.json';

const createdEventAccounts: PublicKey[] = [];

interface SolVaultSwapDetails {
  // chain: string;
  instruction: SolInstruction;
}

type SolInstruction = {
  program_id: string;
  accounts: RpcAccountMeta[];
  data: string;
};

type RpcAccountMeta = {
  pubkey: string;
  isSigner: boolean;
  isWritable: boolean;
};

interface SolanaVaultSwapExtraParameters {
  chain: 'Solana';
  from: string;
  event_data_account: string;
  input_amount: string;
  refund_parameters: ChannelRefundParameters;
  from_token_account?: string;
}

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

// Temporary before the SDK implements this.
export async function executeSolVaultSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
) {
  const destChain = chainFromAsset(destAsset);

  const solanaVaultDataAccount = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));
  const swapEndpointDataAccount = new PublicKey(
    getContractAddress('Solana', 'SWAP_ENDPOINT_DATA_ACCOUNT'),
  );
  const whaleKeypair = getSolWhaleKeyPair();

  const connection = getSolConnection();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const VaultIdl: any = await getSolanaVaultIdl();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const SwapEndpointIdl: any = await getSolanaSwapEndpointIdl();

  // TODO: To use to compare with the returned data from the broker API.
  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(SwapEndpointIdl as SwapEndpoint);
  const vaultProgram = new anchor.Program<Vault>(VaultIdl as Vault);

  // TODO: To use to compare with the returned data from the broker API.
  const fetchedDataAccount = await vaultProgram.account.dataAccount.fetch(solanaVaultDataAccount);
  const aggKey = fetchedDataAccount.aggKey;

  const newEventAccountKeypair = Keypair.generate();
  createdEventAccounts.push(newEventAccountKeypair.publicKey);

  const amountToSwap = amountToFineAmount(
    amount ?? defaultAssetAmounts(srcAsset),
    assetDecimals(srcAsset),
  );

  // TODO: Is this needed for Vault swaps to DOT?
  // const destinationAddress =
  //   destChain === Chains.Polkadot ? decodeDotAddressForContract(destAddress) : destAddress;

  // TODO: Unify this with BTC (& maybe later EVM) vault swaps.
  await using chainflip = await getChainflipApi();
  const brokerUri = '//BROKER_1';
  const broker = createStateChainKeypair(brokerUri);

  const refundParams: ChannelRefundParameters = {
    retry_duration: 0,
    refund_address: decodeSolAddress(whaleKeypair.publicKey.toBase58()),
    min_price: '0x0',
  };

  const extraParameters: SolanaVaultSwapExtraParameters = {
    chain: 'Solana',
    from: decodeSolAddress(whaleKeypair.publicKey.toBase58()),
    event_data_account: decodeSolAddress(newEventAccountKeypair.publicKey.toBase58()),
    input_amount: amountToSwap,
    refund_parameters: refundParams,
    // TODO: Hardcoded for Sol atm
    from_token_account: undefined,
  };

  console.log('amountToSwap', amountToSwap);
  console.log('0x + Number(amountToSwap).toString(16)', '0x' + Number(amountToSwap).toString(16));

  const vaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    { chain: chainFromAsset(srcAsset), asset: stateChainAssetFromAsset(srcAsset) },
    { chain: chainFromAsset(destAsset), asset: stateChainAssetFromAsset(destAsset) },
    destAddress,
    0, // broker_commission
    extraParameters, // extra_parameters
    // TODO: To pass ccm metadata
    null, // channel_metadata
    null, // boost_fee
    null, // affiliates
    null, // dca_parameters
  )) as unknown as SolVaultSwapDetails;

  console.log('vaultSwapDetails:', vaultSwapDetails);

  // TODO: Assert also that programId is the Vault one.
  // assert.strictEqual(vaultSwapDetails.chain, 'Solana');
  console.log('vaultSwapDetails.instruction:', vaultSwapDetails.instruction.accounts);

  // Iterate over vaultSwapDetails.instruction.accounts which is AccountMeta[] but it
  // should be converted into the web 3 AccountMeta[]
  const keys: AccountMeta[] = [];
  for (const account of vaultSwapDetails.instruction.accounts) {
    keys.push({
      pubkey: new PublicKey(account.pubkey),
      isSigner: account.isSigner,
      isWritable: account.isWritable,
    });
  }

  const transaction = new Transaction();
  const instruction = new TransactionInstruction({
    keys,
    programId: new PublicKey(vaultSwapDetails.instruction.program_id),
    data: Buffer.from(vaultSwapDetails.instruction.data, 'hex'),
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
  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(SwapEndpointIdl as SwapEndpoint);
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
