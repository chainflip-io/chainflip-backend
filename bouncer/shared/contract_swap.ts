import {
  Asset,
  executeSwap,
  executeCall,
  ExecuteSwapParams,
  ExecuteCallParams,
  approveVault,
} from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import {
  chainFromAsset,
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  getEthContractAddress,
  observeCcmReceived,
} from './utils';
import { getNextEthNonce } from './send_eth';
import { getBalance } from './get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';
import { getDestinationAddress } from '../tests/swapping';

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): Promise<any> {
  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  const destChain = chainFromAsset(destAsset);

  const nonce = await getNextEthNonce();

  let receipt;
  if (!messageMetadata) {
    receipt = await executeSwap(
      {
        destChain,
        destAsset,
        // It is important that this is large enough to result in
        // an amount larger than existential (e.g. on Polkadot):
        amount: srcAsset === 'USDC' ? '500000000' : '1000000000000000000',
        destAddress,
        ...(srcAsset !== 'ETH' ? { srcAsset } : {}),
      } as ExecuteSwapParams,
      {
        signer: wallet,
        nonce,
        network: 'localnet',
        vaultContractAddress: getEthContractAddress('VAULT'),
        ...(srcAsset !== 'ETH' ? { srcTokenContractAddress: getEthContractAddress(srcAsset) } : {}),
      },
    );
  } else {
    receipt = await executeCall(
      {
        destChain,
        destAsset,
        // It is important that this is large enough to result in
        // an amount larger than existential (e.g. on Polkadot):
        amount: srcAsset === 'USDC' ? '500000000' : '1000000000000000000',
        destAddress,
        ...(srcAsset !== 'ETH' ? { srcAsset } : {}),
        gasAmount: messageMetadata.gas_budget.toString(),
        message: messageMetadata.message,
      } as ExecuteCallParams,
      {
        signer: wallet,
        nonce,
        network: 'localnet',
        vaultContractAddress: getEthContractAddress('VAULT'),
        ...(srcAsset !== 'ETH' ? { srcTokenContractAddress: getEthContractAddress(srcAsset) } : {}),
      },
    );
  }

  return receipt;
}

export async function performSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  messageMetadata?: CcmDepositMetadata,
) {
  const api = await getChainflipApi();

  const { address, tag } = await getDestinationAddress(
    sourceAsset,
    destAsset,
    undefined,
    messageMetadata,
    `[contract `,
  );

  try {
    const oldBalance = await getBalance(destAsset, address);
    console.log(`Old balance: ${oldBalance}`);
    console.log(
      `Executing (${sourceAsset}) contract swap to(${destAsset}) ${address}. Current balance: ${oldBalance}`,
    );
    // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const receipt = await executeContractSwap(sourceAsset, destAsset, address, messageMetadata);
    await observeEvent('swapping:SwapScheduled', api, (event) => {
      if ('vault' in event[5]) {
        return event[5].vault.txHash === receipt.transactionHash;
      }
      // Otherwise it was a swap scheduled by requesting a deposit address
      return false;
    });
    console.log(`Successfully observed event: swapping: SwapScheduled`);

    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(sourceAsset, destAsset, address, messageMetadata)
      : Promise.resolve();

    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, address, oldBalance),
      ccmEventEmitted,
    ]);
    console.log(`${tag} Swap success! New balance: ${newBalance}!`);
  } catch (err) {
    throw new Error(`${tag} ${err}`);
  }
}

export async function approveTokenVault(srcAsset: 'FLIP' | 'USDC', amount: string) {
  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  const nonce = await getNextEthNonce();
  return approveVault(
    {
      amount,
      srcAsset,
    },
    {
      signer: wallet,
      nonce,
      network: 'localnet',
      vaultContractAddress: getEthContractAddress('VAULT'),
      srcTokenContractAddress: getEthContractAddress(srcAsset),
    },
  );
}
