import { Asset } from '@chainflip-io/cli';
import { doPerformSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import { observeFetch, observeBadEvents, sleep } from '../shared/utils';

async function testDepositEthereum(sourceAsset: Asset, destAsset: Asset) {
  const swapParams = await testSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    ' EthereumDepositTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(sourceAsset, swapParams.depositAddress);

  await doPerformSwap(swapParams, `[${sourceAsset}->${destAsset} EthereumDepositTest2]`);
}

async function testDuplicatedDeposit(destAsset: Asset) {
  let stopObserving = false;
  const sourceAsset = 'ETH';

  const swapParams = await testSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    ' DuplicatedDepositTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  const observingSwapScheduled = observeBadEvents(
    'swapping:SwapScheduled',
    () => stopObserving,
    (event) => {
      if ('DepositChannel' in event.data.origin) {
        const channelMatches =
          Number(event.data.origin.DepositChannel.channelId) === swapParams.channelId;
        const assetMatches = sourceAsset === (event.data.sourceAsset.toUpperCase() as Asset);
        return channelMatches && assetMatches;
      }
      return false;
    },
  );

  await observeFetch(sourceAsset, swapParams.depositAddress);

  // Arbitrary time value that should be enough to determine that another swap has not been triggered.
  // Trying to witness the fetch BroadcastSuccess is just unnecessarily complicated here.
  await sleep(100000);

  stopObserving = true;
  await observingSwapScheduled;
}

export async function testEthereumDeposits() {
  console.log('=== Testing Deposits ===');

  const depositTests = Promise.all([
    testDepositEthereum('ETH', 'DOT'),
    testDepositEthereum('FLIP', 'BTC'),
  ]);

  const duplicatedDepositTest = Promise.all([
    testDuplicatedDeposit('DOT'),
    testDuplicatedDeposit('BTC'),
    testDuplicatedDeposit('FLIP'),
    testDuplicatedDeposit('USDC'),
  ]);

  await Promise.all([depositTests, duplicatedDepositTest]);

  console.log('=== Deposit Tests completed ===');
}
