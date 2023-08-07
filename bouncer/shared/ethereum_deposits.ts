import Web3 from 'web3';
import {
  Asset,
  chainContractIds,
  assetChains,
  assetContractIds,
  assetDecimals,
} from '@chainflip-io/cli';
import { doPerformSwap } from '../shared/perform_swap';
import { prepareSwap, testSwap } from '../shared/swapping';
import {
  observeFetch,
  observeBadEvents,
  sleep,
  observeEvent,
  getChainflipApi,
  getEthContractAddress,
  decodeDotAddressForContract,
  defaultAssetAmounts,
  amountToFineAmount,
  runWithTimeout,
} from '../shared/utils';
import { signAndSendTxEth } from './send_eth';
import { getCFTesterAbi } from './eth_abis';

const cfTesterAbi = await getCFTesterAbi();

async function testDepositEthereum(srcAsset: Asset, dstAsset: Asset) {
  const swapParams = await testSwap(
    srcAsset,
    dstAsset,
    undefined,
    undefined,
    ' EthereumDepositTest',
  );

  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(srcAsset, swapParams.depositAddress);

  await doPerformSwap(swapParams, `[${srcAsset}->${dstAsset} EthereumDepositTest2]`);
}

async function testSuccessiveDeposits(dstAsset: Asset) {
  let stopObserving = false;
  const srcAsset = 'ETH';

  const swapParams = await testSwap(
    srcAsset,
    dstAsset,
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
        const assetMatches = srcAsset === (event.data.srcAsset.toUpperCase() as Asset);
        return channelMatches && assetMatches;
      }
      return false;
    },
  );

  await observeFetch(srcAsset, swapParams.depositAddress);

  // Arbitrary time value that should be enough to determine that another swap has not been triggered.
  // Trying to witness the fetch BroadcastSuccess is just unnecessarily complicated here.
  await sleep(100000);

  stopObserving = true;
  await observingSwapScheduled;
}

// Not supporting BTC to avoid adding more unnecessary complexity with address encoding.
async function testTxMultipleContractSwaps(srcAsset: Asset, dstAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(srcAsset, dstAsset);
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  const cfTesterAddress = getEthContractAddress('CFTESTER');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);
  const amount = BigInt(amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals[srcAsset]));
  const numSwaps = 2;
  const txData = cfTesterContract.methods
    .multipleContractSwap(
      chainContractIds[assetChains[dstAsset]],
      dstAsset === 'DOT' ? decodeDotAddressForContract(destAddress) : destAddress,
      assetContractIds[dstAsset],
      getEthContractAddress(srcAsset),
      amount,
      '0x',
      numSwaps,
    )
    .encodeABI();
  const receipt = await signAndSendTxEth(
    cfTesterAddress,
    (amount * BigInt(numSwaps)).toString(),
    txData,
  );

  let eventCounter = 0;
  let stopObserve = false;

  const observingEvent = observeEvent(
    'swapping:SwapScheduled',
    await getChainflipApi(),
    (event) => {
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
    () => stopObserve,
  );

  while (eventCounter === 0) {
    await sleep(2000);
  }
  console.log(`${tag} Successfully observed event: swapping: SwapScheduled`);

  // Wait some more time after the first event to ensure another one is not emited
  await sleep(30000);

  stopObserve = true;
  await observingEvent;
}

export async function testEthereumDeposits() {
  console.log('=== Testing Deposits ===');

  const depositTests = Promise.all([
    testDepositEthereum('ETH', 'DOT'),
    testDepositEthereum('FLIP', 'BTC'),
  ]);

  const duplicatedDepositTest = Promise.all([
    testSuccessiveDeposits('DOT'),
    testSuccessiveDeposits('BTC'),
    testSuccessiveDeposits('FLIP'),
    testSuccessiveDeposits('USDC'),
  ]);

  const multipleTxSwapsTest = Promise.all([
    testTxMultipleContractSwaps('ETH', 'DOT'),
    testTxMultipleContractSwaps('ETH', 'FLIP'),
  ]);

  await Promise.all([depositTests, duplicatedDepositTest, multipleTxSwapsTest]);

  console.log('=== Deposit Tests completed ===');
}
