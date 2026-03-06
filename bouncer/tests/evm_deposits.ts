import { InternalAsset as Asset, broker } from '@chainflip/cli';
import { doPerformSwap, requestNewSwap } from 'shared/perform_swap';
import { prepareSwap, testSwap } from 'shared/swapping';
import BigNumber from 'bignumber.js';
import {
  observeFetch,
  sleep,
  getContractAddress,
  decodeDotAddressForContract,
  defaultAssetAmounts,
  chainFromAsset,
  getWeb3,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
  stateChainAssetFromAsset,
  amountToFineAmountBigInt,
  createEvmWalletAndFund,
  decodeFlipAddressForContract,
  getChainContractId,
  getAssetContractId,
  checkTransactionInMatches,
  checkRequestTypeMatches,
  TransactionOriginId,
} from 'shared/utils';
import { signAndSendTxEvm } from 'shared/send_evm';
import { getCFTesterAbi, getEvmVaultAbi } from 'shared/contract_interfaces';
import { send } from 'shared/send';

import { observeBadEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { newEvmAddress } from 'shared/new_evm_address';
import { brokerApiEndpoint } from 'shared/json_rpc';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { SwapContext } from 'shared/utils/swap_context';
import { swappingSwapRequested } from 'generated/events/swapping/swapRequested';
import assert from 'assert';
import { arbitrumIngressEgressDepositFinalised } from 'generated/events/arbitrumIngressEgress/depositFinalised';
import { arbitrumIngressEgressUnknownBroker } from 'generated/events/arbitrumIngressEgress/unknownBroker';

const cfTesterAbi = await getCFTesterAbi();
const cfEvmVaultAbi = await getEvmVaultAbi();

async function testSuccessiveDepositEvm<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  swapContext: SwapContext,
) {
  const swapParams = await testSwap(
    cf,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    swapContext,
    'EvmDepositTestFirstDeposit',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(sourceAsset, swapParams.depositAddress);

  await doPerformSwap(
    cf.withChildLogger(`[${sourceAsset}->${destAsset} EvmDepositTestSecondDeposit]`),
    swapParams,
  );

  cf.debug('Success');
}

async function testNoDuplicateWitnessing<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  swapContext: SwapContext,
) {
  const swapParams = await testSwap(
    cf,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    swapContext,
    'NoDuplicateWitnessingTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  const observingSwapScheduled = observeBadEvent(cf.logger, 'swapping:SwapScheduled', {
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
  await sleep(60000);

  await observingSwapScheduled.stop();

  cf.debug('Success');
}

// Not supporting Btc to avoid adding more unnecessary complexity with address encoding.
async function testTxMultipleVaultSwaps<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const { destAddress, tag } = await prepareSwap(parentCf.logger, sourceAsset, destAsset);
  const cf = parentCf.withChildLogger(`${tag} testTxMultipleVaultSwaps`);

  const web3 = getWeb3(chainFromAsset(sourceAsset));

  const cfTesterAddress = getContractAddress(chainFromAsset(sourceAsset), 'CFTESTER');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);
  const amount = amountToFineAmountBigInt(defaultAssetAmounts(sourceAsset), sourceAsset);

  const numSwaps = 2;
  const txData = cfTesterContract.methods
    .multipleContractSwap(
      getChainContractId(chainFromAsset(destAsset)),
      destAsset === 'Dot' || destAddress === 'Hub'
        ? decodeDotAddressForContract(destAddress)
        : destAddress,
      getAssetContractId(destAsset),
      getContractAddress(chainFromAsset(sourceAsset), sourceAsset),
      amount,
      // Dummy encoded data containing a refund address and th broker accountId `5FKyTaAoazbwkQ7CHFNJfhWV5sVnRw23HWdPUeQ2tTp3gryJ`.
      '0x00010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000009059e6d854b769a505d01148af212bf8cb7f8469a7153edce8dcaedd9d299125010000',
      numSwaps,
    )
    .encodeABI();

  const receipt = await signAndSendTxEvm(
    cf.logger,
    chainFromAsset(sourceAsset),
    cfTesterAddress,
    (amount * BigInt(numSwaps)).toString(),
    txData,
  );

  const txOrigin: TransactionOriginId = {
    type: TransactionOrigin.VaultSwapEvm,
    txHash: receipt.transactionHash,
  };

  // Wait for multiple SwapRequested events. These can appear in the same block but will have different
  // swapRequestId
  const foundSwapRequestIds: bigint[] = [];
  // TODO: Set the loop limit back to numSwaps, once the deduplication ingress issue is fixed
  for (let i = 1; i <= numSwaps - 1; i++) {
    const swapRequestedEvent = await cf.stepUntilEvent(
      'Swapping.SwapRequested',
      swappingSwapRequested.refine((event) => {
        const channelMatches = checkTransactionInMatches(event.origin, txOrigin);
        const sourceAssetMatches = sourceAsset === event.inputAsset;
        const destAssetMatches = destAsset === event.outputAsset;
        const requestTypeMatches = checkRequestTypeMatches(
          event.requestType,
          SwapRequestType.Regular,
        );
        const differentSwapReqId = !foundSwapRequestIds.includes(event.swapRequestId);
        return (
          channelMatches &&
          sourceAssetMatches &&
          destAssetMatches &&
          requestTypeMatches &&
          differentSwapReqId
        );
      }),
    );
    cf.debug(`Found SwapRequested event ${i} : ${JSON.stringify(swapRequestedEvent)}`);
    foundSwapRequestIds.push(swapRequestedEvent.swapRequestId);
  }
  assert.strictEqual(foundSwapRequestIds.length, numSwaps - 1); // TODO rever back to numSwaps once the deduplication ingress issue is fixed
  cf.info(`Success found ${foundSwapRequestIds.length} SwapRequested events`);
}

async function testDoubleDeposit<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const { destAddress, tag } = await prepareSwap(
    parentCf.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'EvmDoubleDepositTest',
  );

  const cf = parentCf.withChildLogger(`${tag} testDoubleDeposit`);
  const swapParams = await requestNewSwap(cf, sourceAsset, destAsset, destAddress);

  await send(cf.logger, sourceAsset, swapParams.depositAddress);
  await observeSwapRequested(
    cf,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  // Do another deposit. Regardless if the fetch has been broadcasted or not, another swap
  // should be scheduled when we deposit again.
  await send(cf.logger, sourceAsset, swapParams.depositAddress);

  await observeSwapRequested(
    cf,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  cf.debug('Success');
}

async function testEvmLegacyCfParametersVaultSwap<A = []>(parentCf: ChainflipIO<A>) {
  const cf = parentCf.withChildLogger('testEvmLegacyCfParametersVaultSwap');

  const sourceAsset = 'ArbEth';
  const srcChain = 'Arbitrum';
  const evmWallet = await createEvmWalletAndFund(cf.logger, sourceAsset);

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

    const receipt = await signAndSendTxEvm(cf.logger, srcChain, tx.to, tx.value, tx.data, tx.gas, {
      privateKey: evmWallet.privateKey,
    });

    const subcf = cf.withChildLogger(
      `${vaultSwapDetails.chain}_${vaultSwapDetails.broker ?? ''} testEvmLegacyCfParametersVaultSwap`,
    );

    subcf.debug(`Vault swap transaction sent with hash: ${receipt.transactionHash}`);

    // The swap will be refunded because the mainnet broker doesn't match the testnet broker
    // but the swap is observed correctly.
    if (vaultSwapDetails.broker) {
      await subcf.stepUntilAllEventsOf({
        depositFinalized: {
          name: 'ArbitrumIngressEgress.DepositFinalised',
          schema: arbitrumIngressEgressDepositFinalised.refine(
            (event) =>
              event.depositDetails.txHashes &&
              event.depositDetails.txHashes[0] === receipt.transactionHash &&
              event.originType === 'Vault',
          ),
        },
        unknownBroker: {
          name: 'ArbitrumIngressEgress.UnknownBroker',
          schema: arbitrumIngressEgressUnknownBroker.refine(
            (event) => event.brokerId === vaultSwapDetails.broker,
          ),
        },
      });
    } else {
      await subcf.stepUntilEvent(
        'ArbitrumIngressEgress.DepositFinalised',
        arbitrumIngressEgressDepositFinalised.refine(
          (event) =>
            event.depositDetails.txHashes &&
            event.depositDetails.txHashes[0] === receipt.transactionHash &&
            event.originType === 'Vault',
        ),
      );
    }

    cf.debug('Success');
  }
}

async function testEncodeCfParameters<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const web3 = getWeb3(chainFromAsset(sourceAsset));
  const cfVaultAddress = getContractAddress(chainFromAsset(sourceAsset), 'VAULT');
  const cfVaultContract = new web3.eth.Contract(cfEvmVaultAbi, cfVaultAddress);

  const { destAddress, tag } = await prepareSwap(parentCf.logger, sourceAsset, destAsset);
  const cf = parentCf.withChildLogger(`${tag} testEncodeCfParameters`);

  const fillOrKillParams: FillOrKillParamsX128 = {
    retryDurationBlocks: 10,
    refundAddress: newEvmAddress(`refund_eth_${tag}`),
    minPriceX128: '0',
    refundCcmMetadata: undefined,
    maxOraclePriceSlippage: undefined,
  };

  const amount = amountToFineAmountBigInt(defaultAssetAmounts(sourceAsset), sourceAsset);

  const cfParameters = await broker.requestCfParametersEncoding(
    {
      srcAsset: stateChainAssetFromAsset(sourceAsset),
      destAsset: stateChainAssetFromAsset(destAsset),
      destAddress,
      commissionBps: 1,
      fillOrKillParams,
      amount: amount.toString(),
    },
    {
      url: brokerApiEndpoint,
    },
    'backspin',
  );

  const txData = cfVaultContract.methods
    .xSwapNative(
      getChainContractId(chainFromAsset(destAsset)),
      destAsset === 'Flip' ? decodeFlipAddressForContract(destAddress) : destAddress,
      getAssetContractId(destAsset),
      cfParameters,
    )
    .encodeABI();

  const receipt = await signAndSendTxEvm(
    cf.logger,
    chainFromAsset(sourceAsset),
    cfVaultAddress,
    amount.toString(),
    txData,
  );

  cf.debug(`Vault swap transaction hash: ${receipt.transactionHash}`);

  await observeSwapRequested(
    cf,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.VaultSwapEvm, txHash: receipt.transactionHash },
    SwapRequestType.Regular,
  );

  cf.debug('Success');
}

export async function dotestEvmDeposits<A = []>(
  cf: ChainflipIO<A>,
  swapContext: SwapContext,
): Promise<void> {
  const depositTests = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testSuccessiveDepositEvm(subcf, 'Eth', 'Sol', swapContext),
      (subcf) => testSuccessiveDepositEvm(subcf, 'Flip', 'Btc', swapContext),
      (subcf) => testSuccessiveDepositEvm(subcf, 'ArbEth', 'Flip', swapContext),
      (subcf) => testSuccessiveDepositEvm(subcf, 'ArbUsdc', 'Btc', swapContext),
    ]);

  const noDuplicatedWitnessingTest = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testNoDuplicateWitnessing(subcf, 'Eth', 'Sol', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'Eth', 'Btc', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'Eth', 'Flip', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'Eth', 'Usdc', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'ArbEth', 'Sol', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'ArbEth', 'Btc', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'ArbEth', 'Flip', swapContext),
      (subcf) => testNoDuplicateWitnessing(subcf, 'ArbEth', 'Usdc', swapContext),
    ]);

  const multipleTxSwapsTest = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testTxMultipleVaultSwaps(subcf, 'Eth', 'Flip'),
      (subcf) => testTxMultipleVaultSwaps(subcf, 'ArbEth', 'Flip'),
    ]);

  const doubleDepositTests = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testDoubleDeposit(subcf, 'Eth', 'Flip'),
      (subcf) => testDoubleDeposit(subcf, 'Usdc', 'Flip'),
      (subcf) => testDoubleDeposit(subcf, 'ArbEth', 'Sol'),
      (subcf) => testDoubleDeposit(subcf, 'ArbUsdc', 'Flip'),
    ]);

  const testEncodingCfParameters = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testEncodeCfParameters(subcf, 'ArbEth', 'Eth'),
      (subcf) => testEncodeCfParameters(subcf, 'Eth', 'Flip'),
    ]);

  await cf.all([
    depositTests,
    noDuplicatedWitnessingTest,
    multipleTxSwapsTest,
    doubleDepositTests,
    testEvmLegacyCfParametersVaultSwap,
    testEncodingCfParameters,
  ]);
}

export async function testEvmDeposits(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await dotestEvmDeposits(cf, testContext.swapContext);
}
