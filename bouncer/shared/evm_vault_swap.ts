import * as anchor from '@coral-xyz/anchor';
// import NodeWallet from '@coral-xyz/anchor/dist/cjs/nodewallet';￼
import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  Chain,
  assetConstants,
} from '@chainflip/cli';
import { HDNodeWallet } from 'ethers';
import { randomBytes } from 'crypto';
import { PublicKey, sendAndConfirmTransaction, Keypair } from '@solana/web3.js';
import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from '@solana/spl-token';
import {
  observeBalanceIncrease,
  getContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  stateChainAssetFromAsset,
  createEvmWalletAndFund,
  newAddress,
  evmChains,
  getSolWhaleKeyPair,
  getSolConnection,
  chainContractId,
  decodeSolAddress,
  decodeDotAddressForContract,
  sleep,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from './new_swap';
import { SwapContext, SwapStatus } from './swap_context';

import { SwapEndpoint } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/swap_endpoint';
import { Vault } from '../../contract-interfaces/sol-program-idls/v1.0.0-swap-endpoint/vault';
import { getSolanaSwapEndpointIdl, getSolanaVaultIdl } from './contract_interfaces';

// Workaround because of anchor issue
const { BN } = anchor.default;

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

const createdEventAccounts: PublicKey[] = [];

export async function executeVaultSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  wallet?: HDNodeWallet,
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);

  const refundAddress = await newAddress(sourceAsset, randomBytes(32).toString('hex'));
  const fokParams = fillOrKillParams ?? {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };

  const evmWallet = wallet ?? (await createEvmWalletAndFund(sourceAsset));

  if (erc20Assets.includes(sourceAsset)) {
    // Doing effectively infinite approvals to make sure it doesn't fail.
    // eslint-disable-next-line @typescript-eslint/no-use-before-define
    await approveTokenVault(
      sourceAsset,
      (BigInt(amountToFineAmount(amountToSwap, assetDecimals(sourceAsset))) * 100n).toString(),
      evmWallet,
    );
  }

  const networkOptions = {
    signer: evmWallet,
    network: 'localnet',
    vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getContractAddress(srcChain, sourceAsset),
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
      amount: amountToFineAmount(amountToSwap, assetDecimals(sourceAsset)),
      destAddress,
      srcAsset: stateChainAssetFromAsset(sourceAsset),
      srcChain,
      ccmParams: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
        ccmAdditionalData: messageMetadata.ccmAdditionalData,
      },
      // The SDK will encode these parameters and the ccmAdditionalData
      // into the `cfParameters` field for the vault swap.
      boostFeeBps,
      fillOrKillParams: fokParams,
      dcaParams,
      beneficiaries: undefined,
    } as ExecuteSwapParams,
    networkOptions,
    txOptions,
  );

  return receipt;
}
// Temporary before the SDK implements this.
export async function executeSolVaultSwap(
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

  const amount = new BN(amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)));

  let cfParameters;

  if (messageMetadata) {
    // TODO: Currently manually encoded. To use SDK/BrokerApi.
    switch (destChain) {
      case Chains.Ethereum:
      case Chains.Arbitrum:
        cfParameters =
          '0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000';
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
          '0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000';
        break;
      case Chains.Polkadot:
        cfParameters =
          '0x000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000';
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
            amount,
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
            amount,
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

export type VaultSwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  destAddress: string;
  txHash: string;
};

export async function performVaultSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag = '',
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  log = true,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<VaultSwapParams> {
  const tag = swapTag ?? '';
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);
  const srcChain = chainFromAsset(sourceAsset);

  try {
    let wallet;
    let txHash: string;
    let sourceAddress: string;

    if (evmChains.includes(srcChain)) {
      // Generate a new wallet for each vault swap to prevent nonce issues when running in parallel
      // with other swaps via deposit channels.
      wallet = await createEvmWalletAndFund(sourceAsset);
      sourceAddress = wallet!.address.toLowerCase();
    } else {
      sourceAddress = getSolWhaleKeyPair().publicKey.toBase58();
    }

    const oldBalance = await getBalance(destAsset, destAddress);
    if (log) {
      console.log(`${tag} Old balance: ${oldBalance}`);
      console.log(
        `${tag} Executing (${sourceAsset}) vault swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
      );
    }

    // TODO: Temporary before the SDK implements this.
    if (evmChains.includes(srcChain)) {
      // To uniquely identify the VaultSwap, we need to use the TX hash. This is only known
      // after sending the transaction, so we send it first and observe the events afterwards.
      // There are still multiple blocks of safety margin inbetween before the event is emitted
      const receipt = await executeVaultSwap(
        sourceAsset,
        destAsset,
        destAddress,
        messageMetadata,
        amountToSwap,
        boostFeeBps,
        fillOrKillParams,
        dcaParams,
        wallet,
      );
      txHash = receipt.hash;
      sourceAddress = wallet!.address.toLowerCase();
    } else {
      txHash = await executeSolVaultSwap(sourceAsset, destAsset, destAddress, messageMetadata);
      sourceAddress = decodeSolAddress(getSolWhaleKeyPair().publicKey.toBase58());
    }
    swapContext?.updateStatus(swapTag, SwapStatus.VaultContractExecuted);

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
export async function approveTokenVault(sourceAsset: Asset, amount: string, wallet: HDNodeWallet) {
  if (!erc20Assets.includes(sourceAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${sourceAsset}`);
  }

  const chain = chainFromAsset(sourceAsset as Asset);

  await approveVault(
    {
      amount,
      srcChain: chain as Chain,
      srcAsset: stateChainAssetFromAsset(sourceAsset) as SCAsset,
    },
    {
      signer: wallet,
      network: 'localnet',
      vaultContractAddress: getContractAddress(chain, 'VAULT'),
      srcTokenContractAddress: getContractAddress(chain, sourceAsset),
    },
    // This is run with fresh addresses to prevent nonce issues
    {
      nonce: 0,
    },
  );
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