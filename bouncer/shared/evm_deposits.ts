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
  observeSwapScheduled,
} from '../shared/utils';
import { signAndSendTxEvm } from './send_evm';
import { getCFTesterAbi } from './contract_interfaces';
import { send } from './send';

import { observeEvent, observeBadEvent } from './utils/substrate';

const cfTesterAbi = await getCFTesterAbi();

async function testSuccessiveDepositEvm(sourceAsset: Asset, destAsset: Asset) {
  const swapParams = await testSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    undefined,
    ' EvmDepositTestFirstDeposit',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(sourceAsset, swapParams.depositAddress);

  await doPerformSwap(swapParams, `[${sourceAsset}->${destAsset} EvmDepositTestSecondDeposit]`);
}

async function testNoDuplicateWitnessing(sourceAsset: Asset, destAsset: Asset) {
  const swapParams = await testSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    undefined,
    ' NoDuplicateWitnessingTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  const observingSwapScheduled = observeBadEvent('swapping:SwapScheduled', {
    test: (event) => {
      if ('DepositChannel' in event.data.origin) {
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
async function testTxMultipleContractSwaps(sourceAsset: Asset, destAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(sourceAsset, destAsset);

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
      '0x',
      numSwaps,
    )
    .encodeABI();
  const receipt = await signAndSendTxEvm(
    chainFromAsset(sourceAsset),
    cfTesterAddress,
    (amount * BigInt(numSwaps)).toString(),
    txData,
  );

  let eventCounter = 0;
  const observingEvent = observeEvent('swapping:SwapScheduled', {
    test: (event) => {
      if (
        'Vault' in event.data.origin &&
        event.data.origin.Vault.txHash === receipt.transactionHash
      ) {
        if (++eventCounter > 1) {
          throw new Error('Multiple swap scheduled events detected');
        }
      }
      return false;
    },
    abortable: true,
  });

  while (eventCounter === 0) {
    await sleep(2000);
  }
  console.log(`${tag} Successfully observed event: swapping: SwapScheduled`);

  // Wait some more time after the first event to ensure another one is not emited
  await sleep(30000);

  observingEvent.stop();
  await observingEvent.event;
}

async function testDoubleDeposit(sourceAsset: Asset, destAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    ' EvmDoubleDepositTest',
  );
  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  let swapScheduledHandle = observeSwapScheduled(sourceAsset, destAsset, swapParams.channelId);

  await send(sourceAsset, swapParams.depositAddress, defaultAssetAmounts(sourceAsset));

  // Wait for SC to schedule a swap
  await swapScheduledHandle;

  // Do another deposit. Regardless of the fetch having been bradcasted or not, another swap
  // should be scheduled when we deposit again.
  swapScheduledHandle = observeSwapScheduled(sourceAsset, destAsset, swapParams.channelId);

  await send(sourceAsset, swapParams.depositAddress, defaultAssetAmounts(sourceAsset));
  await swapScheduledHandle;
}

export async function testEvmDeposits() {
  console.log('=== Testing EVM Deposits ===');

  const depositTests = Promise.all([
    testSuccessiveDepositEvm('Eth', 'Dot'),
    testSuccessiveDepositEvm('Flip', 'Btc'),
    testSuccessiveDepositEvm('ArbEth', 'Dot'),
    testSuccessiveDepositEvm('ArbUsdc', 'Btc'),
  ]);

  const noDuplicatedWitnessingTest = Promise.all([
    testNoDuplicateWitnessing('Eth', 'Dot'),
    testNoDuplicateWitnessing('Eth', 'Btc'),
    testNoDuplicateWitnessing('Eth', 'Flip'),
    testNoDuplicateWitnessing('Eth', 'Usdc'),
    testNoDuplicateWitnessing('ArbEth', 'Dot'),
    testNoDuplicateWitnessing('ArbEth', 'Btc'),
    testNoDuplicateWitnessing('ArbEth', 'Flip'),
    testNoDuplicateWitnessing('ArbEth', 'Usdc'),
  ]);

  const multipleTxSwapsTest = Promise.all([
    testTxMultipleContractSwaps('Eth', 'Dot'),
    testTxMultipleContractSwaps('Eth', 'Flip'),
    testTxMultipleContractSwaps('ArbEth', 'Dot'),
    testTxMultipleContractSwaps('ArbEth', 'Flip'),
  ]);

  const doubleDepositTests = Promise.all([
    testDoubleDeposit('Eth', 'Dot'),
    testDoubleDeposit('Usdc', 'Flip'),
    testDoubleDeposit('ArbEth', 'Dot'),
    testDoubleDeposit('ArbUsdc', 'Btc'),
  ]);

  await Promise.all([
    depositTests,
    noDuplicatedWitnessingTest,
    multipleTxSwapsTest,
    doubleDepositTests,
  ]);

  console.log('=== EVM Deposit Test completed ===');
}
