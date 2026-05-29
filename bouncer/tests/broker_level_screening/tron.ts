import {
  newAssetAddress,
  sleep,
  chainFromAsset,
  observeBalanceIncrease,
  observeFetch,
  Chains,
  getTronWebClient,
  getEncodedTronAddress,
  Asset,
  chainGasAsset,
} from 'shared/utils';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBalance } from 'shared/get_balance';
import { executeTronVaultSwap } from 'shared/vault_swap/tron_vault_swap';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { tronIngressEgressTransactionRejectedByBrokerEvent } from 'generated/events/tronIngressEgress/transactionRejectedByBroker';
import { tronIngressEgressDepositFinalisedEvent } from 'generated/events/tronIngressEgress/depositFinalised';
import { send } from 'shared/send';

/**
 * Wait for the Tron deposit contract to be deployed at the given address.
 */
async function waitForDepositContractDeployment(depositAddress: string) {
  const MAX_RETRIES = 100;
  const tronWeb = getTronWebClient();
  const encodedAddress = getEncodedTronAddress(depositAddress);
  let contractDeployed = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const contract = await tronWeb.trx.getContract(encodedAddress);
    if (contract && contract.code_hash) {
      contractDeployed = true;
      break;
    }
    await sleep(6000);
  }
  if (!contractDeployed) {
    throw new Error(`Tron contract not deployed at address ${depositAddress} within timeout!`);
  }
}

async function waitForTronTransactionRejection<A = []>(cf: ChainflipIO<A>, txHash: string) {
  const resultEvent = await cf.stepUntilOneEventOf({
    transactionRejected: tronIngressEgressTransactionRejectedByBrokerEvent.refine(
      (event) => event.txId.txHashes && event.txId.txHashes[0] === txHash,
    ),
    depositFinalized: tronIngressEgressDepositFinalisedEvent.refine(
      (event) => event.depositDetails.txHashes && event.depositDetails.txHashes[0] === txHash,
    ),
  });

  if (resultEvent.key === 'depositFinalized') {
    throw new Error(
      `Failed to reject Tron tx ${txHash}. The transaction was ingressed instead of being rejected.
       It might be because the deposit monitor was late in reporting the tx and the transaction ended up being swapped instead`,
    );
  }
}

export async function testTron<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const cf = parentCf.withChildLogger(`${sourceAsset}_BrokerLevelScreening_testTron`);
  cf.info(`Testing broker level screening for Tron ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Tron) {
    throw new Error('Expected chain to be Tron');
  }

  const destinationAddressForBtc = await newAssetAddress('Btc');
  cf.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const tronRefundAddress = await newAssetAddress(sourceAsset);
  const initialRefundAddressBalance = await getBalance(sourceAsset, tronRefundAddress);

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: tronRefundAddress,
    minPriceX128: '0',
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

  if (sourceAsset === chainGasAsset('Tron')) {
    await send(cf.logger, sourceAsset, swapParams.depositAddress);
    cf.debug(`Sent initial ${sourceAsset} tx...`);

    await cf.stepUntilEvent(
      tronIngressEgressDepositFinalisedEvent.refine(
        (event) =>
          event.depositAddress === swapParams.depositAddress &&
          event.channelId === BigInt(swapParams.channelId),
      ),
    );
    await cf.stepOneBlock();

    cf.debug(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // successfully to make sure the Deposit contract is deployed.
    await waitForDepositContractDeployment(swapParams.depositAddress);
  }

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(cf.logger, sourceAsset, swapParams.depositAddress)) as string;
  cf.debug(`Sent ${sourceAsset} tx, hash is ${txHash}`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await waitForTronTransactionRejection(cf, txHash);

  await Promise.all([
    observeBalanceIncrease(cf.logger, sourceAsset, tronRefundAddress, initialRefundAddressBalance),
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  cf.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

export async function testTronVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Tron) {
    throw new Error('Expected chain to be Tron');
  }

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const tronRefundAddress = await newAssetAddress('Trx');

  cf.debug(`Refund address for ${sourceAsset} is ${tronRefundAddress}...`);

  cf.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);
  const txHash = await executeTronVaultSwap(
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
    [],
    tronRefundAddress,
  );
  cf.debug(`Sent ${sourceAsset} (vault swap) tx, hash is ${txHash}`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

  await waitForTronTransactionRejection(cf, txHash);

  let receivedRefund = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, tronRefundAddress);
    if (refundBalance !== '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(`Didn't receive funds refund to address ${tronRefundAddress} within timeout!`);
  }
  cf.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}
