import * as anchor from '@coral-xyz/anchor';

import { InternalAsset as Asset, Chains, assetConstants } from '@chainflip/cli';
import { PublicKey, sendAndConfirmTransaction, Keypair } from '@solana/web3.js';
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
} from './utils';
import { CcmDepositMetadata } from './new_swap';

import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/swap_endpoint';
import { Vault } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/vault';
import { getSolanaSwapEndpointIdl, getSolanaVaultIdl } from './contract_interfaces';

// @ts-expect-error workaround because of anchor issue
const { BN } = anchor.default;

const createdEventAccounts: PublicKey[] = [];

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

  // Using AnchorProvider runs into issues so instead we store the wallet in id.json and then
  // set the ANCHOR_WALLET env. Depending on how the SDK is implemented we can remove this.
  process.env.ANCHOR_WALLET = 'shared/solana_keypair.json';

  const connection = getSolConnection();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const VaultIdl: any = await getSolanaVaultIdl();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const SwapEndpointIdl: any = await getSolanaSwapEndpointIdl();

  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(SwapEndpointIdl as SwapEndpoint);
  const vaultProgram = new anchor.Program<Vault>(VaultIdl as Vault);

  const newEventAccountKeypair = Keypair.generate();
  createdEventAccounts.push(newEventAccountKeypair.publicKey);

  const fetchedDataAccount = await vaultProgram.account.dataAccount.fetch(solanaVaultDataAccount);
  const aggKey = fetchedDataAccount.aggKey;

  const amountToSwap = new BN(
    amountToFineAmount(amount ?? defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)),
  );

  let cfParameters;

  if (messageMetadata) {
    // TODO: Currently manually encoded. To use SDK/BrokerApi.
    switch (destChain) {
      case Chains.Ethereum:
      case Chains.Arbitrum:
        cfParameters =
          '0x000001000000040101010101010101010101010101010101010101010101010101010101010101000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000';
        break;
      default:
        throw new Error(`Unsupported chain: ${destChain}`);
    }
  } else {
    // TODO: Currently manually encoded. To use SDK/BrokerApi.
    switch (destChain) {
      case Chains.Ethereum:
      case Chains.Arbitrum:
        cfParameters =
          '0001000000040101010101010101010101010101010101010101010101010101010101010101000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000';
        break;
      case Chains.Polkadot:
        cfParameters =
          '0001000000010404040404040404040404040404040404040404040404040404040404040404000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000';
        break;
      // TODO: Not supporting BTC for now because the encoding is annoying.
      default:
        throw new Error(`Unsupported chain: ${destChain}`);
    }
  }

  const destinationAddress =
    destChain === Chains.Polkadot ? decodeDotAddressForContract(destAddress) : destAddress;

  const tx =
    srcAsset === 'Sol'
      ? await cfSwapEndpointProgram.methods
          .xSwapNative({
            amount: amountToSwap,
            dstChain: chainContractId(destChain),
            dstAddress: Buffer.from(destinationAddress.slice(2), 'hex'),
            dstToken: assetConstants[destAsset].contractId,
            ccmParameters: messageMetadata
              ? {
                  message: Buffer.from(messageMetadata.message.slice(2), 'hex'),
                  gasAmount: new BN(messageMetadata.gasBudget),
                }
              : null,
            cfParameters: Buffer.from(cfParameters!.slice(2) ?? '', 'hex'),
          })
          .accountsPartial({
            dataAccount: solanaVaultDataAccount,
            aggKey,
            from: whaleKeypair.publicKey,
            eventDataAccount: newEventAccountKeypair.publicKey,
            swapEndpointDataAccount,
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([whaleKeypair, newEventAccountKeypair])
          .transaction()
      : await cfSwapEndpointProgram.methods
          .xSwapToken({
            amount: amountToSwap,
            dstChain: chainContractId(destChain),
            dstAddress: Buffer.from(destinationAddress.slice(2), 'hex'),
            dstToken: assetConstants[destAsset].contractId,
            ccmParameters: messageMetadata
              ? {
                  message: Buffer.from(messageMetadata.message.slice(2), 'hex'),
                  gasAmount: new BN(messageMetadata.gasBudget),
                }
              : null,
            cfParameters: Buffer.from(cfParameters!.slice(2) ?? '', 'hex'),
            decimals: assetDecimals(srcAsset),
          })
          .accountsPartial({
            dataAccount: solanaVaultDataAccount,
            tokenVaultAssociatedTokenAccount: new PublicKey(
              getContractAddress('Solana', 'TOKEN_VAULT_ATA'),
            ),
            from: whaleKeypair.publicKey,
            fromTokenAccount: getAssociatedTokenAddressSync(
              new PublicKey(getContractAddress('Solana', 'SolUsdc')),
              whaleKeypair.publicKey,
              false,
            ),
            eventDataAccount: newEventAccountKeypair.publicKey,
            swapEndpointDataAccount,
            tokenSupportedAccount: new PublicKey(
              getContractAddress('Solana', 'SolUsdcTokenSupport'),
            ),
            tokenProgram: TOKEN_PROGRAM_ID,
            mint: new PublicKey(getContractAddress('Solana', 'SolUsdc')),
            systemProgram: anchor.web3.SystemProgram.programId,
          })
          .signers([whaleKeypair, newEventAccountKeypair])
          .transaction();
  const txHash = await sendAndConfirmTransaction(connection, tx, [
    whaleKeypair,
    newEventAccountKeypair,
  ]);

  console.log('tx', txHash);
  return txHash;
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
