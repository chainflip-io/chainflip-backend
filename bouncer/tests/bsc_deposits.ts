import { brokerEncodeCfParameters } from 'shared/utils/broker_api';
import { doPerformSwap, requestNewSwap } from 'shared/perform_swap';
import { prepareSwap, testSwap } from 'shared/swapping';
import {
  observeFetch,
  sleep,
  getContractAddress,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
  amountToFineAmountBigInt,
  defaultAssetAmounts,
  chainFromAsset,
  getWeb3,
  getChainContractId,
  getAssetContractId,
  checkTransactionInMatches,
  checkRequestTypeMatches,
  TransactionOriginId,
  decodeDotAddressForContract,
  Chains,
  Asset,
} from 'shared/utils';
import { signAndSendTxEvm } from 'shared/send_evm';
import { getEvmVaultAbi } from 'shared/contract_interfaces';
import { send } from 'shared/send';
import { observeBadEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { newEvmAddress } from 'shared/new_evm_address';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { SwapContext } from 'shared/utils/swap_context';
import { swappingSwapRequested } from 'generated/events/swapping/swapRequested';

const cfEvmVaultAbi = await getEvmVaultAbi();

// ─── Deposit channel tests ────────────────────────────────────────────────────

// Opens a deposit channel, sends funds, and verifies the engine witnesses the
// deposit and fetches (drains) the contract.  A second send to the same channel
// confirms channels remain live after the first deposit.
async function testSuccessiveDepositBsc<A = []>(
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
    'BscDepositTestFirstDeposit',
  );

  // Balance drops to zero once the engine witnesses and fetches the deposit.
  await observeFetch(sourceAsset, swapParams.depositAddress);

  await doPerformSwap(
    cf.withChildLogger(`[${sourceAsset}->${destAsset} BscDepositTestSecondDeposit]`),
    swapParams,
  );

  cf.debug('Success');
}

// Ensures the engine does NOT emit a duplicate SwapScheduled event when the
// same deposit is observed more than once (re-org resilience).
async function testNoDuplicateWitnessingBsc<A = []>(
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
    'BscNoDuplicateWitnessingTest',
  );

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

  // Give the chain enough time to confirm no duplicate witnessing occurs.
  await sleep(60000);

  await observingSwapScheduled.stop();

  cf.debug('Success');
}

// Confirms a second send to the same deposit channel triggers a second
// independent swap, regardless of whether the first fetch has been broadcast.
async function testDoubleDepositBsc<A = []>(
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
    'BscDoubleDepositTest',
  );

  const cf = parentCf.withChildLogger(`${tag} testDoubleDepositBsc`);
  const swapParams = await requestNewSwap(cf, sourceAsset, destAsset, destAddress);

  await send(cf.logger, sourceAsset, swapParams.depositAddress);
  await observeSwapRequested(
    cf,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  // Second deposit to the same channel — a new swap should be scheduled.
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

// ─── Vault swap tests ─────────────────────────────────────────────────────────

// Tests the cf_encode_cf_parameters RPC → xSwapNative vault call flow for BSC.
// Encodes parameters via the broker API, calls the BSC vault contract directly,
// and waits for a SwapRequested event with the matching transaction hash.
async function testEncodeCfParametersBsc<A = []>(
  parentCf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const web3 = getWeb3(chainFromAsset(sourceAsset));
  const cfVaultAddress = getContractAddress(chainFromAsset(sourceAsset), 'VAULT');
  const cfVaultContract = new web3.eth.Contract(cfEvmVaultAbi, cfVaultAddress);

  const { destAddress, tag } = await prepareSwap(parentCf.logger, sourceAsset, destAsset);
  const cf = parentCf.withChildLogger(`${tag} testEncodeCfParametersBsc`);

  const fillOrKillParams: FillOrKillParamsX128 = {
    retryDurationBlocks: 10,
    refundAddress: newEvmAddress(`refund_bsc_${tag}`),
    minPriceX128: '0',
    refundCcmMetadata: undefined,
    maxOraclePriceSlippage: undefined,
  };

  const amount = amountToFineAmountBigInt(defaultAssetAmounts(sourceAsset), sourceAsset);

  const cfParameters = await brokerEncodeCfParameters(
    cf.logger,
    sourceAsset,
    destAsset,
    destAddress,
    1,
    fillOrKillParams,
  );

  const txData = cfVaultContract.methods
    .xSwapNative(
      getChainContractId(chainFromAsset(destAsset)),
      chainFromAsset(destAsset) === Chains.Assethub
        ? decodeDotAddressForContract(destAddress)
        : destAddress,
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

  cf.debug(`BSC vault swap transaction hash: ${receipt.transactionHash}`);

  const txOrigin: TransactionOriginId = {
    type: TransactionOrigin.VaultSwapEvm,
    txHash: receipt.transactionHash,
  };

  await cf.stepUntilEvent(
    'Swapping.SwapRequested',
    swappingSwapRequested.refine(
      (event) =>
        checkTransactionInMatches(event.origin, txOrigin) &&
        sourceAsset === event.inputAsset &&
        destAsset === event.outputAsset &&
        checkRequestTypeMatches(event.requestType, SwapRequestType.Regular),
    ),
  );

  cf.debug('Success');
}

// ─── Test runner ──────────────────────────────────────────────────────────────

export async function dotestBscDeposits<A = []>(
  cf: ChainflipIO<A>,
  swapContext: SwapContext,
): Promise<void> {
  // Basic deposit channel tests: native Bnb and ERC-20 BscUsdt.
  const depositTests = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testSuccessiveDepositBsc(subcf, 'Bnb', 'Eth', swapContext),
      (subcf) => testSuccessiveDepositBsc(subcf, 'BscUsdt', 'Btc', swapContext),
    ]);

  // Duplicate witnessing guards for both asset types.
  const noDuplicatedWitnessingTest = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'Bnb', 'Eth', swapContext),
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'Bnb', 'Btc', swapContext),
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'Bnb', 'Flip', swapContext),
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'Bnb', 'Usdc', swapContext),
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'BscUsdt', 'Eth', swapContext),
      (subcf) => testNoDuplicateWitnessingBsc(subcf, 'BscUsdt', 'Btc', swapContext),
    ]);

  // Double deposit: channel stays live for a second deposit.
  const doubleDepositTests = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testDoubleDepositBsc(subcf, 'Bnb', 'Flip'),
      (subcf) => testDoubleDepositBsc(subcf, 'BscUsdt', 'Flip'),
    ]);

  // Vault swap: encode parameters and call BSC vault directly.
  const vaultSwapTests = (parentCf: ChainflipIO<A>) =>
    parentCf.all([
      (subcf) => testEncodeCfParametersBsc(subcf, 'Bnb', 'Eth'),
      (subcf) => testEncodeCfParametersBsc(subcf, 'Bnb', 'Flip'),
    ]);

  await cf.all([depositTests, noDuplicatedWitnessingTest, doubleDepositTests, vaultSwapTests]);
}

export async function testBscDeposits(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await dotestBscDeposits(cf, testContext.swapContext);
}
