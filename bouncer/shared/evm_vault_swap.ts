import { InternalAsset as Asset, broker } from '@chainflip/cli';
import { Contract, HDNodeWallet } from 'ethers';
import { randomBytes } from 'crypto';
import assert from 'assert';
import Web3 from 'web3';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  stateChainAssetFromAsset,
  createEvmWalletAndFund,
  newAssetAddress,
  decodeDotAddressForContract,
  getEvmEndpoint,
  Chains,
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { Logger } from 'shared/utils/logger';
import { getErc20abi } from 'shared/contract_interfaces';
import { brokerApiEndpoint } from './json_rpc';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeEvmVaultSwap(
  logger: Logger,
  brokerUri: string,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerCommissionBps: number = 0,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  wallet?: HDNodeWallet,
  affiliateFees: {
    accountAddress: string;
    commissionBps: number;
  }[] = [],
  optionalRefundAddress?: string,
) {
  const srcChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);
  const refundAddress =
    optionalRefundAddress ?? (await newAssetAddress(sourceAsset, randomBytes(32).toString('hex')));
  const fineAmount = amountToFineAmount(amountToSwap, assetDecimals(sourceAsset));
  const evmWallet = wallet ?? (await createEvmWalletAndFund(logger, sourceAsset));

  if (erc20Assets.includes(sourceAsset)) {
    // Doing effectively infinite approvals to make sure it doesn't fail.
    // eslint-disable-next-line @typescript-eslint/no-use-before-define
    await approveEvmTokenVault(
      sourceAsset,
      (BigInt(amountToFineAmount(amountToSwap, assetDecimals(sourceAsset))) * 100n).toString(),
      evmWallet,
    );
  }

  logger.trace('Requesting vault swap parameter encoding');
  const vaultSwapDetails = await broker.requestSwapParameterEncoding(
    {
      srcAsset: stateChainAssetFromAsset(sourceAsset),
      srcAddress: evmWallet.address,
      destAsset: stateChainAssetFromAsset(destAsset),
      destAddress:
        destChain === Chains.Polkadot || destChain === Chains.Assethub
          ? decodeDotAddressForContract(destAddress)
          : destAddress,
      commissionBps: brokerCommissionBps,
      ccmParams: messageMetadata && {
        message: messageMetadata.message,
        gasBudget: messageMetadata.gasBudget,
        ccmAdditionalData: messageMetadata.ccmAdditionalData,
      },
      fillOrKillParams: fillOrKillParams ?? {
        retryDurationBlocks: 0,
        refundAddress,
        minPriceX128: '0',
      },
      maxBoostFeeBps: boostFeeBps ?? 0,
      amount: fineAmount,
      dcaParams: dcaParams && {
        numberOfChunks: dcaParams.numberOfChunks,
        chunkIntervalBlocks: dcaParams.chunkIntervalBlocks,
      },
      affiliates: affiliateFees.map((fee) => ({
        account: fee.accountAddress,
        commissionBps: fee.commissionBps,
      })),
    },
    {
      url: brokerApiEndpoint,
    },
    'backspin',
  );

  assert(
    vaultSwapDetails.chain === 'Ethereum' || vaultSwapDetails.chain === 'Arbitrum',
    `Expected chain to be Ethereum or Arbitrum, got ${vaultSwapDetails.chain}`,
  );

  const web3 = new Web3(getEvmEndpoint(srcChain));
  const tx = {
    to: vaultSwapDetails.to,
    data: vaultSwapDetails.calldata,
    value: vaultSwapDetails.value.toString(),
    gas: srcChain === 'Arbitrum' ? 32000000 : 5000000,
  };

  logger.trace('Signing and Sending EVM vault swap transaction');
  const signedTx = await web3.eth.accounts.signTransaction(tx, evmWallet.privateKey);
  const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);

  return receipt.transactionHash;
}

export async function approveEvmTokenVault(
  sourceAsset: Asset,
  amount: string,
  wallet: HDNodeWallet,
) {
  if (!erc20Assets.includes(sourceAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${sourceAsset}`);
  }

  const erc20abi = await getErc20abi();
  const chain = chainFromAsset(sourceAsset);
  const tokenContractAddress = getContractAddress(chain, sourceAsset);
  const sourceTokenContract = new Contract(tokenContractAddress, erc20abi, wallet);

  const approvalTx = await sourceTokenContract.approve(
    getContractAddress(chain, 'VAULT'),
    amount,
    // This is run with fresh addresses to prevent nonce issues
    { nonce: 0 },
  );
  await approvalTx.wait();
}
