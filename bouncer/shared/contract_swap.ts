import * as anchor from '@coral-xyz/anchor';
// import NodeWallet from '@coral-xyz/anchor/dist/cjs/nodewallet';

import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  InternalAsset,
  Chain,
} from '@chainflip/cli';
import { HDNodeWallet, Wallet, getDefaultProvider } from 'ethers';
import { PublicKey, sendAndConfirmTransaction, Keypair } from '@solana/web3.js';
import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from '@solana/spl-token';
import {
  observeBalanceIncrease,
  getContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  stateChainAssetFromAsset,
  chainGasAsset,
  evmChains,
  getSolWhaleKeyPair,
  getSolConnection,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';
import { send } from './send';
import { SwapContext, SwapStatus } from './swap_context';

import VaultIdl from '../../contract-interfaces/sol-program-idls/v1.0.0/vault.json';
import SwapEndpointIdl from '../../contract-interfaces/sol-program-idls/v1.0.0/swap_endpoint.json';
import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.0/types/swap_endpoint';
import { Vault } from '../../contract-interfaces/sol-program-idls/v1.0.0/types/vault';

// Workaround because of anchor issue
const { BN } = anchor.default;

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  wallet: HDNodeWallet,
  messageMetadata?: CcmDepositMetadata,
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(srcAsset);
  const destChain = chainFromAsset(destAsset);

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getContractAddress(srcChain, srcAsset),
  } as const;
  const txOptions = {
    // This is run with fresh addresses to prevent nonce issues. Will be 1 for ERC20s.
    gasLimit: srcChain === Chains.Arbitrum ? 10000000n : 200000n,
  } as const;

  const receipt = await executeSwap(
    {
      destChain,
      destAsset: stateChainAssetFromAsset(destAsset),
      // It is important that this is large enough to result in
      // an amount larger than existential (e.g. on Polkadot):
      amount: amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)),
      destAddress,
      srcAsset: stateChainAssetFromAsset(srcAsset),
      srcChain,
      ccmParams: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
        cfParameters: messageMetadata.cfParameters,
      },
    } as ExecuteSwapParams,
    networkOptions,
    txOptions,
  );

  return receipt;
}

// Temporary before the SDK implements this.
export async function executeSolContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
) {
  const destChain = chainFromAsset(destAsset);

  // const solanaSwapEndpointId = new PublicKey(getContractAddress('Solana', 'SWAP_ENDPOINT'));
  const solanaVaultDataAccount = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));
  const swapEndpointDataAccount = new PublicKey(
    getContractAddress('Solana', 'SWAP_ENDPOINT_DATA_ACCOUNT'),
  );
  const whaleKeypair = getSolWhaleKeyPair();

  // We should just be able to do this instead but it's not working...
  // const wallet = new NodeWallet(whaleKeypair);
  // const provider = new anchor.AnchorProvider(connection, wallet, {
  //   commitment: 'processed',
  // });
  // const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(SwapEndpointIdl, provider);
  // const vaultProgram = new anchor.Program<Vault>(VaultIdl, provider);

  // The current workaround requires having the wallet in a id.json and then set the ANCHOR_WALLET env.
  // TODO: Depending on how the SDK is implemented we can remove this.
  process.env.ANCHOR_WALLET = 'shared/solana_keypair.json';

  const connection = getSolConnection();
  const cfSwapEndpointProgram = new anchor.Program<SwapEndpoint>(SwapEndpointIdl as SwapEndpoint);
  const vaultProgram = new anchor.Program<Vault>(VaultIdl as Vault);

  const newEventAccountKeypair = Keypair.generate();
  const fetchedDataAccount = await vaultProgram.account.dataAccount.fetch(solanaVaultDataAccount);
  const aggKey = fetchedDataAccount.aggKey;

  const tx =
    srcAsset === 'Sol'
      ? await cfSwapEndpointProgram.methods
          .xSwapNative({
            amount: new BN(
              amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)),
            ),
            dstChain: Number(destChain),
            dstAddress: Buffer.from(destAddress),
            dstToken: Number(stateChainAssetFromAsset(destAsset)),
            ccmParameters: messageMetadata
              ? {
                  message: Buffer.from(messageMetadata.message.slice(2), 'hex'),
                  gasAmount: new BN(messageMetadata.gasBudget),
                }
              : null,
            cfParameters: Buffer.from(messageMetadata?.cfParameters?.slice(2) ?? '', 'hex'),
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
            amount: new BN(
              amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)),
            ),
            dstChain: Number(destChain),
            dstAddress: Buffer.from(destAddress),
            dstToken: Number(stateChainAssetFromAsset(destAsset)),
            ccmParameters: messageMetadata
              ? {
                  message: Buffer.from(messageMetadata.message.slice(2), 'hex'),
                  gasAmount: new BN(messageMetadata.gasBudget),
                }
              : null,
            cfParameters: Buffer.from(messageMetadata?.cfParameters?.slice(2) ?? '', 'hex'),
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
export type ContractSwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  destAddress: string;
  txHash: string;
};

export async function performSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag = '',
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  log = true,
): Promise<ContractSwapParams> {
  const tag = swapTag ?? '';

  const srcChain = chainFromAsset(sourceAsset);
  let wallet;

  try {
    if (evmChains.includes(srcChain)) {
      // Generate a new wallet for each contract swap to prevent nonce issues when running in parallel
      // with other swaps via deposit channels.
      const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
      if (mnemonic === '') {
        throw new Error('Failed to create random mnemonic');
      }
      wallet = Wallet.fromPhrase(mnemonic).connect(getDefaultProvider(getEvmEndpoint(srcChain)));

      // Fund new key with native asset and asset to swap.
      await send(chainGasAsset(srcChain) as InternalAsset, wallet.address);
      await send(sourceAsset, wallet.address);

      if (erc20Assets.includes(sourceAsset)) {
        // Doing effectively infinite approvals to make sure it doesn't fail.
        // eslint-disable-next-line @typescript-eslint/no-use-before-define
        await approveTokenVault(
          sourceAsset,
          (
            BigInt(
              amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset)),
            ) * 100n
          ).toString(),
          wallet,
        );
      }
    }
    swapContext?.updateStatus(swapTag, SwapStatus.ContractApproved);

    const oldBalance = await getBalance(destAsset, destAddress);
    if (log) {
      console.log(`${tag} Old balance: ${oldBalance}`);
      console.log(
        `${tag} Executing (${sourceAsset}) contract swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
      );
    }

    let txHash: string;
    let sourceAddress: string;

    // TODO: Temporary before the SDK implements this.
    if (evmChains.includes(srcChain)) {
      // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
      // after sending the transaction, so we send it first and observe the events afterwards.
      // There are still multiple blocks of safety margin inbetween before the event is emitted
      const receipt = await executeContractSwap(
        sourceAsset,
        destAsset,
        destAddress,
        wallet!,
        messageMetadata,
      );
      txHash = receipt.hash;
      sourceAddress = wallet!.address.toLowerCase();
    } else {
      txHash = await executeSolContractSwap(
        sourceAsset,
        destAsset,
        destAddress,
        // wallet!,
        messageMetadata,
      );
      sourceAddress = getSolWhaleKeyPair().publicKey.toBase58();
    }

    swapContext?.updateStatus(swapTag, SwapStatus.ContractExecuted);

    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata, sourceAddress)
      : Promise.resolve();

    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);
    if (log) {
      console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    }
    swapContext?.updateStatus(swapTag, SwapStatus.Success);
    return {
      sourceAsset,
      destAsset,
      destAddress,
      txHash,
    };
  } catch (err) {
    console.error('err:', err);
    swapContext?.updateStatus(swapTag, SwapStatus.Failure);
    if (err instanceof Error) {
      console.log(err.stack);
    }
    throw new Error(`${tag} ${err}`);
  }
}
export async function approveTokenVault(srcAsset: Asset, amount: string, wallet: HDNodeWallet) {
  if (!erc20Assets.includes(srcAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${srcAsset}`);
  }

  const chain = chainFromAsset(srcAsset as Asset);

  await approveVault(
    {
      amount,
      srcChain: chain as Chain,
      srcAsset: stateChainAssetFromAsset(srcAsset) as SCAsset,
    },
    {
      signer: wallet,
      network: 'localnet',
      vaultContractAddress: getContractAddress(chain, 'VAULT'),
      srcTokenContractAddress: getContractAddress(chain, srcAsset),
    },
    // This is run with fresh addresses to prevent nonce issues
    {
      nonce: 0,
    },
  );
}
