import {
  newAssetAddress,
  sleep,
  chainGasAsset,
  isWithinOnePercent,
  getWeb3,
  chainFromAsset,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
  getChainContractId,
  amountToFineAmount,
  assetDecimals,
  Chain,
  Asset,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { executeEvmVaultSwap } from 'shared/vault_swap/evm_vault_swap';
import { newCcmMetadata } from 'shared/swapping';
import { ChainflipIO, WithBrokerAccount, WithLpAccount } from 'shared/utils/chainflip_io';
import { liquidityProviderLiquidityDepositAddressReadyEvent } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { assetBalancesAccountCreditedEvent } from 'generated/events/assetBalances/accountCredited';
import { ethereumIngressEgressDepositFinalisedEvent } from 'generated/events/ethereumIngressEgress/depositFinalised';
import { arbitrumIngressEgressDepositFinalisedEvent } from 'generated/events/arbitrumIngressEgress/depositFinalised';
import { ethereumIngressEgressTransactionRejectedByBrokerEvent } from 'generated/events/ethereumIngressEgress/transactionRejectedByBroker';
import { arbitrumIngressEgressTransactionRejectedByBrokerEvent } from 'generated/events/arbitrumIngressEgress/transactionRejectedByBroker';

/**
 * Wait for the Deposit contract to be deployed.
 */
async function waitForDepositContractDeployment<A = []>(
  cf: ChainflipIO<A>,
  chain: Chain,
  depositAddress: string,
) {
  switch (chain) {
    case 'Arbitrum':
    case 'Ethereum':
      break;
    default:
      throw new Error(`Unsupported evm chain ${chain}`);
  }

  const MAX_RETRIES = 100;
  const web3 = getWeb3(chain);
  let contractDeployed = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const bytecode = await web3.eth.getCode(depositAddress);
    if (bytecode && bytecode !== '0x') {
      contractDeployed = true;
      break;
    }
    await sleep(6000);
  }
  if (!contractDeployed) {
    throw new Error(`${chain} contract not deployed at address ${depositAddress} within timeout!`);
  }
  cf.debug(`Found deployed contract at ${depositAddress}!`);

  // wait two SC blocks to make sure that the contract deployment + fetch has been witnessed and processed on the SC
  // TODO: we should instead use events or SC-state for this
  await sleep(12000);
}

async function waitForEvmTransactionRejection<A = []>(
  cf: ChainflipIO<A>,
  chain: Chain,
  txHash: string,
) {
  let resultEvent;
  if (chain === 'Ethereum') {
    resultEvent = await cf.stepUntilOneEventOf({
      transactionRejected: ethereumIngressEgressTransactionRejectedByBrokerEvent.refine(
        (event) => event.txId.txHashes && event.txId.txHashes[0] === txHash,
      ),
      depositFinalized: ethereumIngressEgressDepositFinalisedEvent.refine(
        (event) => event.depositDetails.txHashes && event.depositDetails.txHashes[0] === txHash,
      ),
    });
  } else if (chain === 'Arbitrum') {
    resultEvent = await cf.stepUntilOneEventOf({
      transactionRejected: arbitrumIngressEgressTransactionRejectedByBrokerEvent.refine(
        (event) => event.txId.txHashes && event.txId.txHashes[0] === txHash,
      ),
      depositFinalized: arbitrumIngressEgressDepositFinalisedEvent.refine(
        (event) => event.depositDetails.txHashes && event.depositDetails.txHashes[0] === txHash,
      ),
    });
  } else {
    throw Error('Unsupported broker level screening EVM chain');
  }

  if (resultEvent.key === 'depositFinalized') {
    throw new Error(
      `Failed to reject ${chain} tx ${txHash}. The transaction was ingressed instead of being rejected.
       It might be because the deposit monitor was late in reporting the tx and the transaction ended up being swapped instead`,
    );
  }
}

export async function testEvm<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  const cf = parentCf.withChildLogger(`${sourceAsset}_BrokerLevelScreening_TestEvm`);
  cf.info(`Testing broker level screening for Evm ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);

  if (chain !== 'Arbitrum' && chain !== 'Ethereum') {
    throw new Error(
      `testEvm in BLS test only usable for Ethereum and Arbitrum! Found chain: ${chain}`,
    );
  }

  const destinationAddressForBtc = await newAssetAddress('Btc');

  cf.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const ethereumRefundAddress = await newAssetAddress(sourceAsset, undefined, undefined, ccmRefund);
  const initialRefundAddressBalance = await getBalance(sourceAsset, ethereumRefundAddress);

  const refundCcmMetadata = ccmRefund ? await newCcmMetadata(sourceAsset) : undefined;

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: ethereumRefundAddress,
    minPriceX128: '0',
    refundCcmMetadata,
  };

  const swapParams = await requestNewSwap(
    cf,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  if (sourceAsset === chainGasAsset(chain)) {
    await send(cf.logger, sourceAsset, swapParams.depositAddress);
    cf.debug(`Sent initial ${sourceAsset} tx...`);

    if (chain === 'Ethereum') {
      await cf.stepUntilEvent(
        ethereumIngressEgressDepositFinalisedEvent.refine(
          (event) =>
            event.depositAddress === swapParams.depositAddress &&
            event.channelId === BigInt(swapParams.channelId),
        ),
      );
    } else if (chain === 'Arbitrum') {
      await cf.stepUntilEvent(
        arbitrumIngressEgressDepositFinalisedEvent.refine(
          (event) =>
            event.depositAddress === swapParams.depositAddress &&
            event.channelId === BigInt(swapParams.channelId),
        ),
      );
    }

    await cf.stepOneBlock();

    cf.debug(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // successfully to make sure the Deposit contract is deployed.
    await waitForDepositContractDeployment(cf, chain, swapParams.depositAddress);
  }

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(cf.logger, sourceAsset, swapParams.depositAddress))
    .transactionHash as string;
  cf.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  // Observe the TransactionRejectedByBroker event
  await waitForEvmTransactionRejection(cf, chain, txHash);

  await Promise.all([
    observeBalanceIncrease(
      cf.logger,
      sourceAsset,
      ethereumRefundAddress,
      initialRefundAddressBalance,
    ),
    ccmEventEmitted,
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  cf.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

export async function testEvmVaultSwap<A extends WithBrokerAccount>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const cf = parentCf.withChildLogger(`${sourceAsset}_BrokerLevelScreening_testEvmVaultSwap`);

  const chain = chainFromAsset(sourceAsset);

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  cf.debug(`Refund address for ${sourceAsset} is ${ethereumRefundAddress}...`);

  cf.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);
  const txHash = await executeEvmVaultSwap(
    cf,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    0,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    [],
    ethereumRefundAddress,
  );
  cf.debug(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

  await waitForEvmTransactionRejection(cf, chain, txHash);

  let receivedRefund = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    if (refundBalance !== '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds refund to address ${ethereumRefundAddress} within timeout!`,
    );
  }
  cf.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

export async function testEvmLiquidityDeposit<A extends WithLpAccount>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
) {
  // setup access to chainflip api and lp
  await using chainflip = await getChainflipApi();
  const cf = parentCf.withChildLogger(
    `${sourceAsset}_BrokerLevelScreening_testEvmLiquidityDeposit`,
  );
  const lp = cf.requirements.account.keypair;

  const chain = chainFromAsset(sourceAsset);

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset}...`);
  const MAX_RETRIES = 120;

  // Get existing LP refund address of //LP_1 for `sourceAsset`
  /* eslint-disable  @typescript-eslint/no-explicit-any */
  const addressReponse = (
    await chainflip.query.liquidityProvider.liquidityRefundAddress(
      lp.address,
      getChainContractId(chainFromAsset(sourceAsset)),
    )
  ).toJSON() as any;
  if (addressReponse === undefined) {
    throw new Error(`There was now refund address for ${sourceAsset} for the LP.`);
  }

  let ethereumRefundAddress;
  if (chain === 'Ethereum') {
    ethereumRefundAddress = addressReponse.eth;
  } else if (chain === 'Arbitrum') {
    ethereumRefundAddress = addressReponse.arb;
  } else {
    throw new Error('Unsupported Evm chain');
  }
  cf.debug(`refund address is: ${ethereumRefundAddress}`);

  // Create new LP deposit address for //LP_1
  cf.debug('Requesting ' + sourceAsset + ' deposit address');
  const depositAddressReadyEvent = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityProvider.requestLiquidityDepositAddress(sourceAsset, null),
    expectedEvent: liquidityProviderLiquidityDepositAddressReadyEvent.refine(
      (event) => event.asset === sourceAsset && event.accountId === lp.address,
    ),
  });

  const depositAddress = depositAddressReadyEvent.depositAddress.address;
  const depositChannelId = depositAddressReadyEvent.channelId;

  cf.debug(`Got deposit address: ${depositAddress}`);

  if (sourceAsset === chainGasAsset(chain)) {
    // The first tx cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.

    const amount = '3';
    await send(cf.logger, sourceAsset, depositAddress, amount);
    cf.debug(`Sent initial ${sourceAsset} tx...`);

    if (chain === 'Ethereum') {
      await cf.stepUntilEvent(
        ethereumIngressEgressDepositFinalisedEvent.refine(
          (event) =>
            event.depositAddress === depositAddress && event.channelId === depositChannelId,
        ),
      );
    } else if (chain === 'Arbitrum') {
      await cf.stepUntilEvent(
        arbitrumIngressEgressDepositFinalisedEvent.refine(
          (event) =>
            event.depositAddress === depositAddress && event.channelId === depositChannelId,
        ),
      );
    }
    cf.debug(`Initial deposit ${sourceAsset} received...`);

    const observeAccountCreditedEvent = await cf.stepUntilEvent(
      assetBalancesAccountCreditedEvent.refine(
        (event) =>
          event.asset === sourceAsset &&
          event.accountId === lp.address &&
          isWithinOnePercent(
            event.amountCredited,
            BigInt(amountToFineAmount(String(amount), assetDecimals(sourceAsset))),
          ),
      ),
    );
    cf.debug(`Account credited for ${observeAccountCreditedEvent.asset}...`);
    await waitForDepositContractDeployment(cf, chain, depositAddress);
  }

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(cf.logger, sourceAsset, depositAddress)).transactionHash as string;
  cf.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await waitForEvmTransactionRejection(cf, chain, txHash);

  let receivedRefund = false;

  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    const depositAddressBalance = await getBalance(sourceAsset, depositAddress);
    if (refundBalance !== '0' && depositAddressBalance === '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds liquidity deposit refund to ${ethereumRefundAddress} within timeout!`,
    );
  }

  cf.info(`Marked ${sourceAsset} LP deposit was rejected and refunded 👍.`);
}
