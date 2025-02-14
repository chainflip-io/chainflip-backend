import Web3 from 'web3';
import { InternalAsset as Asset } from '@chainflip/cli';
import { doPerformSwap, requestNewSwap } from '../shared/perform_swap';
import { prepareSwap, testSwap } from '../shared/swapping';
import {
  observeFetch,
  sleep,
  getContractAddress,
  decodeDotAddressForContract,
  defaultAssetAmounts,
  amountToFineAmount,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  chainContractId,
  assetContractId,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from '../shared/utils';
import { signAndSendTxEvm } from '../shared/send_evm';
import { getCFTesterAbi } from '../shared/contract_interfaces';
import { send } from '../shared/send';

import { observeEvent, observeBadEvent } from '../shared/utils/substrate';
import { TestContext } from '../shared/utils/test_context';
import { Logger, throwError } from '../shared/utils/logger';

const cfTesterAbi = await getCFTesterAbi();

async function testSuccessiveDepositEvm(
  sourceAsset: Asset,
  destAsset: Asset,
  testContext: TestContext,
) {
  const swapParams = await testSwap(
    testContext.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    testContext.swapContext,
    'EvmDepositTestFirstDeposit',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(sourceAsset, swapParams.depositAddress);

  await doPerformSwap(
    testContext.logger,
    swapParams,
    `[${sourceAsset}->${destAsset} EvmDepositTestSecondDeposit]`,
  );
}

async function testNoDuplicateWitnessing(
  sourceAsset: Asset,
  destAsset: Asset,
  testContext: TestContext,
) {
  const swapParams = await testSwap(
    testContext.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    testContext.swapContext,
    'NoDuplicateWitnessingTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  const observingSwapScheduled = observeBadEvent(testContext.logger, 'swapping:SwapScheduled', {
    test: (event) => {
      if (typeof event.data.origin === 'object' && 'DepositChannel' in event.data.origin) {
        const channelMatches =
          Number(event.data.origin.DepositChannel.channelId) === swapParams.channelId;
        const assetMatches = sourceAsset === (event.data.sourceAsset as Asset);
        return channelMatches && assetMatches;
      }
      return false;
    },
  });

  await observeFetch(sourceAsset, swapParams.depositAddress);

  // Arbitrary time value that should be enough to determine that another swap has not been triggered.
  // Trying to witness the fetch BroadcastSuccess is just unnecessarily complicated here.
  await sleep(100000);

  await observingSwapScheduled.stop();
}

// Not supporting Btc to avoid adding more unnecessary complexity with address encoding.
async function testTxMultipleVaultSwaps(
  parentLogger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const { destAddress, tag } = await prepareSwap(parentLogger, sourceAsset, destAsset);
  const logger = parentLogger.child({ tag });

  const web3 = new Web3(getEvmEndpoint(chainFromAsset(sourceAsset)));

  const cfTesterAddress = getContractAddress(chainFromAsset(sourceAsset), 'CFTESTER');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);
  const amount = BigInt(
    amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset)),
  );
  const numSwaps = 2;
  const txData = cfTesterContract.methods
    .multipleContractSwap(
      chainContractId(chainFromAsset(destAsset)),
      destAsset === 'Dot' ? decodeDotAddressForContract(destAddress) : destAddress,
      assetContractId(destAsset),
      getContractAddress(chainFromAsset(sourceAsset), sourceAsset),
      amount,
      // Dummy encoded data containing a refund address and a broker accountId.
      '0x000001000000000202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000',
      numSwaps,
    )
    .encodeABI();
  const receipt = await signAndSendTxEvm(
    logger,
    chainFromAsset(sourceAsset),
    cfTesterAddress,
    (amount * BigInt(numSwaps)).toString(),
    txData,
  );

  let eventCounter = 0;
  const observingEvent = observeEvent(logger, 'swapping:SwapRequested', {
    test: (event) => {
      if (
        typeof event.data.origin === 'object' &&
        'Vault' in event.data.origin &&
        event.data.origin.Vault.txId.Evm === receipt.transactionHash
      ) {
        if (++eventCounter > 1) {
          throwError(logger, new Error('Multiple swap scheduled events detected'));
        }
      }
      return false;
    },
    abortable: true,
  });

  while (eventCounter === 0) {
    await sleep(2000);
  }

  // Wait some more time after the first event to ensure another one is not emitted
  await sleep(30000);

  observingEvent.stop();
  await observingEvent.event;
}

async function testDoubleDeposit(logger: Logger, sourceAsset: Asset, destAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(
    logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    ' EvmDoubleDepositTest',
  );
  const swapParams = await requestNewSwap(logger, sourceAsset, destAsset, destAddress, tag);

  {
    const swapRequestedHandle = observeSwapRequested(
      logger,
      sourceAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
      SwapRequestType.Regular,
    );

    await send(logger, sourceAsset, swapParams.depositAddress, defaultAssetAmounts(sourceAsset));
    await swapRequestedHandle;
  }

  // Do another deposit. Regardless of the fetch having been broadcasted or not, another swap
  // should be scheduled when we deposit again.
  {
    const swapRequestedHandle = observeSwapRequested(
      logger,
      sourceAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
      SwapRequestType.Regular,
    );

    await send(logger, sourceAsset, swapParams.depositAddress, defaultAssetAmounts(sourceAsset));
    await swapRequestedHandle;
  }
}

export async function testEvmDeposits(testContext: TestContext) {
  const depositTests = Promise.all([
    testSuccessiveDepositEvm('Eth', 'Dot', testContext),
    testSuccessiveDepositEvm('Flip', 'Btc', testContext),
    testSuccessiveDepositEvm('ArbEth', 'Dot', testContext),
    testSuccessiveDepositEvm('ArbUsdc', 'Btc', testContext),
  ]);

  const noDuplicatedWitnessingTest = Promise.all([
    testNoDuplicateWitnessing('Eth', 'Dot', testContext),
    testNoDuplicateWitnessing('Eth', 'Btc', testContext),
    testNoDuplicateWitnessing('Eth', 'Flip', testContext),
    testNoDuplicateWitnessing('Eth', 'Usdc', testContext),
    testNoDuplicateWitnessing('ArbEth', 'Dot', testContext),
    testNoDuplicateWitnessing('ArbEth', 'Btc', testContext),
    testNoDuplicateWitnessing('ArbEth', 'Flip', testContext),
    testNoDuplicateWitnessing('ArbEth', 'Usdc', testContext),
  ]);

  const multipleTxSwapsTest = Promise.all([
    testTxMultipleVaultSwaps(testContext.logger, 'Eth', 'Dot'),
    testTxMultipleVaultSwaps(testContext.logger, 'Eth', 'Flip'),
    testTxMultipleVaultSwaps(testContext.logger, 'ArbEth', 'Dot'),
    testTxMultipleVaultSwaps(testContext.logger, 'ArbEth', 'Flip'),
  ]);

  const doubleDepositTests = Promise.all([
    testDoubleDeposit(testContext.logger, 'Eth', 'Dot'),
    testDoubleDeposit(testContext.logger, 'Usdc', 'Flip'),
    testDoubleDeposit(testContext.logger, 'ArbEth', 'Dot'),
    testDoubleDeposit(testContext.logger, 'ArbUsdc', 'Btc'),
  ]);

  await Promise.all([
    depositTests,
    noDuplicatedWitnessingTest,
    multipleTxSwapsTest,
    doubleDepositTests,
  ]);
}
