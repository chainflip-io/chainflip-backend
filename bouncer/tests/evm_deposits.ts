import Web3 from 'web3';
import { InternalAsset as Asset } from '@chainflip/cli';
import { doPerformSwap, requestNewSwap } from 'shared/perform_swap';
import { prepareSwap, testSwap } from 'shared/swapping';
import BigNumber from 'bignumber.js';
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
  createEvmWalletAndFund,
} from 'shared/utils';
import { signAndSendTxEvm } from 'shared/send_evm';
import { getCFTesterAbi, getEvmVaultAbi } from 'shared/contract_interfaces';
import { send } from 'shared/send';

import { observeEvent, observeBadEvent, getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger, throwError } from 'shared/utils/logger';
import { ChannelRefundParameters } from 'shared/sol_vault_swap';
import { newEvmAddress } from 'shared/new_evm_address';

const cfTesterAbi = await getCFTesterAbi();
const cfEvmVaultAbi = await getEvmVaultAbi();

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
        if (++eventCounter > numSwaps) {
          throwError(logger, new Error('Multiple swap scheduled events detected'));
        }
      }
      return false;
    },
    abortable: true,
    // Don't stop when the event is found.
    stopAfter: 'Never',
    timeoutSeconds: 150,
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

    await send(logger, sourceAsset, swapParams.depositAddress);
    await swapRequestedHandle;
  }

  // Do another deposit. Regardless of the fetch having been broadcasted or not, another swap
  // should be scheduled when we deposit again.
  const swapRequestedHandle = observeSwapRequested(
    logger,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  await send(logger, sourceAsset, swapParams.depositAddress);
  await swapRequestedHandle;
}

async function testEvmLegacyCfParametersVaultSwap(parentLogger: Logger) {
  const logger = parentLogger.child({ tag: 'test' });

  const sourceAsset = 'ArbEth';
  const srcChain = 'Arbitrum';
  const evmWallet = await createEvmWalletAndFund(logger, sourceAsset);
  const web3 = new Web3(getEvmEndpoint(srcChain));

  // Hardcoded payload obtained encoding a Vault Swap with the old Encoding
  const vaultSwapDetailsArray = [
    // https://scan.chainflip.io/swaps/583409
    // https://etherscan.io/tx/0x5cbff756754d7c2b1c935609eb9ca9681480a50c55f946ef45006f89f885e5af#internal
    {
      chain: srcChain,
      calldata:
        '0xdd68734500000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000900000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000020edab591a28ba82b8d2e86c1e3e3548fe1007fb99ef8a46b490c728469cfe928d000000000000000000000000000000000000000000000000000000000000005e009600000094d29e656ad719b348488c516c5ef45a8bb894887cd2d9679dca31c70cb7f5a747000000000000000000000000000000000000000000e0a3208a8748c830691a567b66a9b1d93b93aba308af9004c0c4a20569f30d070000000000',
      value: '0x2E4D8EE1F032000',
      to: getContractAddress(srcChain, 'VAULT'),
      broker: 'cFNx21kQWmr9wsqq29zWM7RpDBKv4bctudEUE6J22Hd4NUUHR',
    },
    // Example with CCM from https://docs.chainflip.io/swapping/integrations/advanced/vault-swaps/evm#1-request-the-encoded-parameters-via-rpc
    {
      chain: srcChain,
      calldata:
        '0x07933dd2000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000003e800000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000014cf0871027a5f984403aefd2fb22831d4bebc11ef00000000000000000000000000000000000000000000000000000000000000000000000000000000000000080011223344556677000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000a00000000cf0871027a5f984403aefd2fb22831d4bebc11ef000000000000000000000000000000000000000000000000000000000000000000009059e6d854b769a505d01148af212bf8cb7f8469a7153edce8dcaedd9d299125000000',
      value: '0x3e8',
      to: getContractAddress(srcChain, 'VAULT'),
      broker: 'cFHtDi8T8QVBoYpxWYkB8up3XAyEyey5bAos3uBwMjz9TtZs1',
    },
    // Example encoded with previous version of localnet (1.9).
    {
      chain: srcChain,
      calldata:
        '0xdd68734500000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000c00000000000000000000000000000000000000000000000000000000000000014ecae5ac7046ebd3d8c811f82dfcef449caf4df00000000000000000000000000000000000000000000000000000000000000000000000000000000000000005e00000000002988897cb44670c37fca6998849f884dfff8da6600009c584c491ff27f16d24b8a61616b1987cabb0d46d4859c4ac6347d0e000000009059e6d854b769a505d01148af212bf8cb7f8469a7153edce8dcaedd9d2991250100000000',
      value: '0x3e8',
      to: getContractAddress(srcChain, 'VAULT'),
    },
  ];

  for (const vaultSwapDetails of vaultSwapDetailsArray) {
    const tx = {
      to: vaultSwapDetails.to,
      data: vaultSwapDetails.calldata,
      value: new BigNumber(vaultSwapDetails.value.slice(2), 16).toString(),
      gas: srcChain === 'Arbitrum' ? 32000000 : 5000000,
    };

    const signedTx = await web3.eth.accounts.signTransaction(tx, evmWallet.privateKey);
    const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);

    logger.info(`Vault swap transaction sent with hash: ${receipt.transactionHash}`);

    const depositFinalisedEvent = observeEvent(
      logger,
      `${chainFromAsset(sourceAsset).toLowerCase()}IngressEgress:DepositFinalised`,
      {
        test: (event) =>
          event.data.originType === 'Vault' &&
          event.data.depositDetails.txHashes[0] === receipt.transactionHash,
      },
    ).event;

    // The swap will be refunded because the mainnet broker doesn't match the testnet broker
    // but the swap is observed correctly.
    const unknownBrokerEvent = vaultSwapDetails.broker
      ? observeEvent(
          logger,
          `${chainFromAsset(sourceAsset).toLowerCase()}IngressEgress:UnknownBroker`,
          {
            test: (event) => event.data.brokerId === vaultSwapDetails.broker,
          },
        ).event
      : Promise.resolve();

    await Promise.all([depositFinalisedEvent, unknownBrokerEvent]);
  }
}

async function testEncodeCfParameters(parentLogger: Logger, sourceAsset: Asset, destAsset: Asset) {
  const web3 = new Web3(getEvmEndpoint(chainFromAsset(sourceAsset)));
  const cfVaultAddress = getContractAddress(chainFromAsset(sourceAsset), 'VAULT');
  const cfVaultContract = new web3.eth.Contract(cfEvmVaultAbi, cfVaultAddress);
  const { destAddress, tag } = await prepareSwap(parentLogger, sourceAsset, destAsset);
  const logger = parentLogger.child({ tag });
  await using chainflip = await getChainflipApi();

  // This will be replaced in PRO-2228 when the SDK is used
  const refundParams: ChannelRefundParameters = {
    retry_duration: 10,
    refund_address: newEvmAddress('refund_eth'),
    min_price: '0x0',
    refund_ccm_metadata: undefined,
    max_oracle_price_slippage: undefined,
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

  const testEncodingCfParameters = Promise.all([
    testEncodeCfParameters(testContext.logger, 'ArbEth', 'Eth'),
    testEncodeCfParameters(testContext.logger, 'Eth', 'Dot'),
  ]);

  await Promise.all([
    depositTests,
    noDuplicatedWitnessingTest,
    multipleTxSwapsTest,
    doubleDepositTests,
    testEvmLegacyCfParametersVaultSwap(testContext.logger),
    testEncodingCfParameters,
  ]);
}
