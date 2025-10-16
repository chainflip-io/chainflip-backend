import axios from 'axios';
import { Chain, Chains, InternalAsset } from '@chainflip/cli';
import Web3 from 'web3';
import { btcClient, sendBtc, sendBtcTransactionWithParent } from 'shared/send_btc';
import {
  newAssetAddress,
  sleep,
  handleSubstrateError,
  chainGasAsset,
  lpMutex,
  createStateChainKeypair,
  isWithinOnePercent,
  amountToFineAmountBigInt,
  getEvmEndpoint,
  chainContractId,
  chainFromAsset,
  ingressEgressPalletForChain,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
  btcClientMutex,
  getBtcClient,
} from 'shared/utils';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import Keyring from 'polkadot/keyring';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBtcBalance } from 'shared/get_btc_balance';
import { TestContext } from 'shared/utils/test_context';
import { getIsoTime, Logger } from 'shared/utils/logger';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { buildAndSendBtcVaultSwap } from 'shared/btc_vault_swap';
import { executeEvmVaultSwap } from 'shared/evm_vault_swap';
import { newCcmMetadata } from 'shared/swapping';

const keyring = new Keyring({ type: 'sr25519' });
const brokerUri = '//BROKER_1';
const broker = keyring.createFromUri(brokerUri);


export async function testSol(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  const logger = testContext.logger;
  logger.info(`Testing broker level screening for Sol ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  if (chain !== Chains.Solana) {
    // This should always be Sol
    throw new Error('Expected chain to be Solana');
  }
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  const destinationAddressForBtc = await newAssetAddress('Btc');

  logger.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const solRefundAddress = await newAssetAddress('Sol', undefined, undefined, ccmRefund);
  const initialRefundAddressBalance = await getBalance(sourceAsset, solRefundAddress);

  const refundCcmMetadata = ccmRefund ? await newCcmMetadata(sourceAsset) : undefined;

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: solRefundAddress,
    minPriceX128: '0',
    refundCcmMetadata,
  };

  const swapParams = await requestNewSwap(
    logger,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  logger.debug(`Sending ${sourceAsset} tx to reject...`);
  const result = await send(logger, sourceAsset, swapParams.depositAddress);
  const txHash = result.transaction.signatures[0] as string;
  logger.debug(`Sent ${sourceAsset} tx, hash is ${txHash}`);

  await reportFunction(txHash);
  logger.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(logger, `${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  await Promise.all([
    observeBalanceIncrease(logger, sourceAsset, solRefundAddress, initialRefundAddressBalance),
    ccmEventEmitted,
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  logger.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}