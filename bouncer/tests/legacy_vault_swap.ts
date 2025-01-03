import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import Web3 from 'web3';
import assert from 'assert';
import {
  amountToFineAmount,
  assetContractId,
  assetDecimals,
  ccmSupportedChains,
  chainContractId,
  chainFromAsset,
  chainGasAsset,
  createEvmWalletAndFund,
  defaultAssetAmounts,
  getContractAddress,
  getEvmEndpoint,
  newAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from '../shared/utils';
import { observeEvent } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { ExecutableTest } from '../shared/executable_test';
import { newVaultSwapCcmMetadata } from '../shared/swapping';
import { getEvmVaultAbi } from '../shared/contract_interfaces';
import { approveEvmTokenVault } from '../shared/evm_vault_swap';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const legacyEvmVaultSwaps = new ExecutableTest('Legacy-EVM-Vault-Swaps', main, 300);

async function legacyEvmVaultSwap(sourceAsset: Asset, destAsset: Asset, ccmSwap: boolean = false) {
  const destAddress = ccmSwap
    ? getContractAddress(chainFromAsset(destAsset), 'CFTESTER')
    : await newAddress(destAsset, randomBytes(32).toString('hex'));
  const destBalanceBefore = await getBalance(destAsset, destAddress);
  const sourceChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amount = amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset));

  // Only EVM have legacy vault swaps
  assert(ccmSupportedChains.includes(sourceChain));

  const web3 = new Web3(getEvmEndpoint(sourceChain));
  const vaultAddress = getContractAddress(sourceChain, 'VAULT');
  const vaultContract = new web3.eth.Contract(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await getEvmVaultAbi()) as any,
    vaultAddress,
  );
  const evmWallet = await createEvmWalletAndFund(sourceAsset);

  const cfParametersList = ['', '0x', 'deadbeef', '0xdeadbeef', 'deadc0de', '0xdeadc0de'];
  const cfParameters = Math.floor(Math.random() * cfParametersList.length);

  if (chainGasAsset(sourceChain) !== sourceAsset) {
    // Doing effectively infinite approvals to make sure it doesn't fail.
    await approveEvmTokenVault(sourceAsset, (BigInt(amount) * 100n).toString(), evmWallet);
  }

  let data;
  if (!ccmSwap) {
    if (chainGasAsset(sourceChain) === sourceAsset) {
      data = vaultContract.methods
        .xSwapNative(
          chainContractId(destChain),
          destAddress,
          assetContractId(destAsset),
          cfParameters,
        )
        .encodeABI();
    } else {
      data = vaultContract.methods
        .xSwapToken(
          chainContractId(destChain),
          destAddress,
          assetContractId(destAsset),
          getContractAddress(sourceChain, sourceAsset),
          Number(amount),
          cfParameters,
        )
        .encodeABI();
    }
  } else {
    const ccmSwapMetadata = await newVaultSwapCcmMetadata(sourceAsset, destAsset);
    if (chainGasAsset(sourceChain) === sourceAsset) {
      data = vaultContract.methods
        .xCallNative(
          chainContractId(destChain),
          destAddress,
          assetContractId(destAsset),
          ccmSwapMetadata.message,
          ccmSwapMetadata.gasBudget,
          cfParameters,
        )
        .encodeABI();
    } else {
      data = vaultContract.methods
        .xCallToken(
          chainContractId(destChain),
          destAddress,
          assetContractId(destAsset),
          ccmSwapMetadata.message,
          ccmSwapMetadata.gasBudget,
          getContractAddress(sourceChain, sourceAsset),
          amount,
          cfParameters,
        )
        .encodeABI();
    }
  }

  const tx = {
    to: vaultAddress,
    data,
    gas: sourceChain === 'Arbitrum' ? 32000000 : 5000000,
    value: chainGasAsset(sourceChain) === sourceAsset ? amount : '0',
  };

  const signedTx = await web3.eth.accounts.signTransaction(tx, evmWallet.privateKey);
  const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);

  legacyEvmVaultSwaps.log(`Vault swap executed, tx hash: ${receipt.transactionHash}`);

  // Look after Swap Requested of data.origin.Vault.tx_hash
  const swapRequestedHandle = observeSwapRequested(
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.VaultSwapEvm, txHash: receipt.transactionHash },
    SwapRequestType.Regular,
  );

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  legacyEvmVaultSwaps.debugLog(`${sourceAsset} swap via vault, swapRequestId: ${swapRequestId}`);

  // Wait for the swap to complete
  await observeEvent(`swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  await observeEvent(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 10,
  }).event;

  legacyEvmVaultSwaps.debugLog(
    `swapRequestId: ${swapRequestId} executed. Waiting for balance to increase.`,
  );
  await observeBalanceIncrease(destAsset, destAddress, destBalanceBefore);
  legacyEvmVaultSwaps.debugLog(`swapRequestId: ${swapRequestId} - swap success`);
}

export async function main() {
  await Promise.all([
    legacyEvmVaultSwap(Assets.Eth, Assets.ArbUsdc),
    legacyEvmVaultSwap(Assets.ArbEth, Assets.Eth, true),
    legacyEvmVaultSwap(Assets.ArbUsdc, Assets.Usdt),
    legacyEvmVaultSwap(Assets.Usdc, Assets.Eth, true),
  ]);
}
