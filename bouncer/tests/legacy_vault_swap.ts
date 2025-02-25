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
import { newVaultSwapCcmMetadata } from '../shared/swapping';
import { getEvmVaultAbi } from '../shared/contract_interfaces';
import { approveEvmTokenVault } from '../shared/evm_vault_swap';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

async function legacyEvmVaultSwap(
  logger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  ccmSwap: boolean = false,
) {
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
  const evmWallet = await createEvmWalletAndFund(logger, sourceAsset);

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

  logger.debug(`Vault swap executed, tx hash: ${receipt.transactionHash}`);

  // Look after Swap Requested of data.origin.Vault.tx_hash
  const swapRequestedHandle = observeSwapRequested(
    logger,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.VaultSwapEvm, txHash: receipt.transactionHash },
    SwapRequestType.Regular,
  );

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  logger.debug(`${sourceAsset} swap via vault, swapRequestId: ${swapRequestId}`);

  // Wait for the swap to complete
  await observeEvent(logger, `swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  await observeEvent(logger, `swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 10,
  }).event;

  logger.debug(`swapRequestId: ${swapRequestId} executed. Waiting for balance to increase.`);
  await observeBalanceIncrease(logger, destAsset, destAddress, destBalanceBefore);
  logger.debug(`swapRequestId: ${swapRequestId} - swap success`);
}

export async function legacyEvmVaultSwaps(testContext: TestContext) {
  await Promise.all([
    legacyEvmVaultSwap(testContext.logger, Assets.Eth, Assets.ArbUsdc),
    legacyEvmVaultSwap(testContext.logger, Assets.ArbEth, Assets.Eth, true),
    legacyEvmVaultSwap(testContext.logger, Assets.ArbUsdc, Assets.Usdt),
    legacyEvmVaultSwap(testContext.logger, Assets.Usdc, Assets.Eth, true),
  ]);
}
