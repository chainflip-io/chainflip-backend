import Web3 from 'web3';
import { InternalAsset as Asset } from '@chainflip/cli';
import { doPerformSwap, requestNewSwap } from 'shared/perform_swap';
import { prepareSwap, testSwap } from 'shared/swapping';
import { Keyring } from '@polkadot/api';
import {
  observeFetch,
  sleep,
  getContractAddress,
  decodeDotAddressForContract,
  defaultAssetAmounts,
  chainFromAsset,
  getEvmEndpoint,
  chainContractId,
  assetContractId,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
  stateChainAssetFromAsset,
  amountToFineAmountBigInt,
} from 'shared/utils';
import { signAndSendTxEvm } from 'shared/send_evm';
import { getCFTesterAbi, getEthScUtilsAbi, getEvmVaultAbi } from 'shared/contract_interfaces';
import { send } from 'shared/send';

import { observeEvent, observeBadEvent, getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger, throwError } from 'shared/utils/logger';
import { ChannelRefundParameters } from 'shared/sol_vault_swap';
import { newEvmAddress } from 'shared/new_evm_address';
import { approveErc20 } from 'shared/approve_erc20';

const cfTesterAbi = await getCFTesterAbi();
const cfEvmVaultAbi = await getEvmVaultAbi();
const cfScUtilsAbi = await getEthScUtilsAbi();

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
    testContext.logger.child({ tag: `[${sourceAsset}->${destAsset} EvmDepositTestSecondDeposit]` }),
    swapParams,
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
  const amount = amountToFineAmountBigInt(defaultAssetAmounts(sourceAsset), sourceAsset);

  const numSwaps = 2;
  const txData = cfTesterContract.methods
    .multipleContractSwap(
      chainContractId(chainFromAsset(destAsset)),
      destAsset === 'Dot' || destAddress === 'Hub'
        ? decodeDotAddressForContract(destAddress)
        : destAddress,
      assetContractId(destAsset),
      getContractAddress(chainFromAsset(sourceAsset), sourceAsset),
      amount,
      // Dummy encoded data containing a refund address and th broker accountId `5FKyTaAoazbwkQ7CHFNJfhWV5sVnRw23HWdPUeQ2tTp3gryJ`.
      '0x00010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000009059e6d854b769a505d01148af212bf8cb7f8469a7153edce8dcaedd9d299125010000',
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

async function testDoubleDeposit(parentLogger: Logger, sourceAsset: Asset, destAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(
    parentLogger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'EvmDoubleDepositTest',
  );
  const logger = parentLogger.child({ tag });
  const swapParams = await requestNewSwap(logger, sourceAsset, destAsset, destAddress);

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

async function testEncodeCfParameters(parentLogger: Logger, sourceAsset: Asset, destAsset: Asset) {
  const web3 = new Web3(getEvmEndpoint(chainFromAsset(sourceAsset)));
  const cfVaultAddress = getContractAddress(chainFromAsset(sourceAsset), 'VAULT');
  const cfVaultContract = new web3.eth.Contract(cfEvmVaultAbi, cfVaultAddress);
  const { destAddress, tag } = await prepareSwap(parentLogger, sourceAsset, destAsset);
  const logger = parentLogger.child({ tag });
  await using chainflip = await getChainflipApi();

  const refundParams: ChannelRefundParameters = {
    retry_duration: 10,
    refund_address: newEvmAddress('refund_eth'),
    min_price: '0x0',
  };

  const cfParameters = (await chainflip.rpc(
    `cf_encode_cf_parameters`,
    new Keyring({ type: 'sr25519' }).createFromUri('//BROKER_1').address,
    { chain: chainFromAsset(sourceAsset), asset: stateChainAssetFromAsset(sourceAsset) },
    { chain: chainFromAsset(destAsset), asset: stateChainAssetFromAsset(destAsset) },
    destAddress,
    1, // broker_comission
    refundParams,
  )) as string;

  const amount = amountToFineAmountBigInt(defaultAssetAmounts(sourceAsset), sourceAsset);

  const txData = cfVaultContract.methods
    .xSwapNative(
      chainContractId(chainFromAsset(destAsset)),
      destAsset === 'Dot' || destAddress === 'Hub'
        ? decodeDotAddressForContract(destAddress)
        : destAddress,
      assetContractId(destAsset),
      cfParameters,
    )
    .encodeABI();

  const receipt = await signAndSendTxEvm(
    logger,
    chainFromAsset(sourceAsset),
    cfVaultAddress,
    amount.toString(),
    txData,
  );

  await observeSwapRequested(
    logger,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.VaultSwapEvm, txHash: receipt.transactionHash },
    SwapRequestType.Regular,
  );
}

async function testDelegate(parentLogger: Logger) {
  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const cfScUtilsContract = new web3.eth.Contract(cfScUtilsAbi, scUtilsAddress);
  const logger = parentLogger.child({ tag: 'Delegate' });

  const amount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');

  console.log("Approving Flip to SC Utils contract for deposit...");
  await approveErc20(
    logger,
    'Flip',
    getContractAddress('Ethereum', 'SC_UTILS'),
    amount.toString(),
  );
  console.log("Approved FLIP");

  const txData = cfScUtilsContract.methods.depositToScGateway(amount.toString(), '0x').encodeABI();

  const receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  console.log('Transaction hash:', receipt.transactionHash);
}

export async function testEvmDeposits(testContext: TestContext) {
  // const depositTests = Promise.all([
  //   testSuccessiveDepositEvm('Eth', 'Dot', testContext),
  //   testSuccessiveDepositEvm('Flip', 'Btc', testContext),
  //   testSuccessiveDepositEvm('ArbEth', 'Dot', testContext),
  //   testSuccessiveDepositEvm('ArbUsdc', 'Btc', testContext),
  // ]);

  // const noDuplicatedWitnessingTest = Promise.all([
  //   testNoDuplicateWitnessing('Eth', 'Dot', testContext),
  //   testNoDuplicateWitnessing('Eth', 'Btc', testContext),
  //   testNoDuplicateWitnessing('Eth', 'Flip', testContext),
  //   testNoDuplicateWitnessing('Eth', 'Usdc', testContext),
  //   testNoDuplicateWitnessing('ArbEth', 'Dot', testContext),
  //   testNoDuplicateWitnessing('ArbEth', 'Btc', testContext),
  //   testNoDuplicateWitnessing('ArbEth', 'Flip', testContext),
  //   testNoDuplicateWitnessing('ArbEth', 'Usdc', testContext),
  // ]);

  // const multipleTxSwapsTest = Promise.all([
  //   testTxMultipleVaultSwaps(testContext.logger, 'Eth', 'Dot'),
  //   testTxMultipleVaultSwaps(testContext.logger, 'Eth', 'Flip'),
  //   testTxMultipleVaultSwaps(testContext.logger, 'ArbEth', 'Dot'),
  //   testTxMultipleVaultSwaps(testContext.logger, 'ArbEth', 'Flip'),
  // ]);

  // const doubleDepositTests = Promise.all([
  //   testDoubleDeposit(testContext.logger, 'Eth', 'Dot'),
  //   testDoubleDeposit(testContext.logger, 'Usdc', 'Flip'),
  //   testDoubleDeposit(testContext.logger, 'ArbEth', 'Dot'),
  //   testDoubleDeposit(testContext.logger, 'ArbUsdc', 'Btc'),
  // ]);

  // const testEncodingCfParameters = Promise.all([
  //   testEncodeCfParameters(testContext.logger, 'ArbEth', 'Eth'),
  //   testEncodeCfParameters(testContext.logger, 'Eth', 'Dot'),
  // ]);

  await Promise.all([
    // depositTests,
    // noDuplicatedWitnessingTest,
    // multipleTxSwapsTest,
    // doubleDepositTests,
    // testEncodingCfParameters,
    testDelegate(testContext.logger),
  ]);
}
