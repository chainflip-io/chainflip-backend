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
} from '../shared/utils';
import { signAndSendTxEth } from './send_eth';
import cfTesterAbi from '../../eth-contract-abis/perseverance-0.9-rc3/CFTester.json';

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

async function testMultipleDeposits(destAsset: Asset) {
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

// Simple double swap test via smart contract call. No need to check all the balances, that's why we have the other tests
// Just a hardcoded swap to simplify the address and chain encoding.
async function testTxMultipleSwaps() {
  const { destAddress, tag } = await prepareSwap('ETH', 'DOT');
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  // performDoubleContractSwap
  const cfTesterAddress = getEthContractAddress('CFTESTER');
  const cfTesterContract = new web3.eth.Contract(cfTesterAbi as any, cfTesterAddress);
  const amount = BigInt(amountToFineAmount(defaultAssetAmounts('ETH'), assetDecimals.ETH));
  const txData = cfTesterContract.methods
    .multipleContractSwap(
      chainContractIds[assetChains.DOT],
      decodeDotAddressForContract(destAddress),
      assetContractIds.DOT,
      getEthContractAddress('ETH'),
      amount,
      '0x',
      2,
    )
    .encodeABI();
  const receipt = await signAndSendTxEth(cfTesterAddress, (amount * 2n).toString(), txData);

  let eventCounter = 0;
  let stopObserve = false;

  const observingEvent = observeEvent(
    'swapping:SwapScheduled',
    await getChainflipApi(),
    (event) => {
      if ('Vault' in event.data.origin) {
        if (event.data.origin.Vault.txHash === receipt.transactionHash) {
          if (eventCounter++ >= 1) {
            throw new Error('Multiple swap scheduled events detected');
          }
        }
      }
      return false;
    },
    () => stopObserve,
  );

  while (eventCounter === 0) {
    await sleep(3000);
  }
  console.log(`${tag} Successfully observed event: swapping: SwapScheduled`);

  // After first event is found, wait some more time to ensure another one is not emited
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
    testMultipleDeposits('DOT'),
    testMultipleDeposits('BTC'),
    testMultipleDeposits('FLIP'),
    testMultipleDeposits('USDC'),
  ]);

  await Promise.all([depositTests, duplicatedDepositTest, testTxMultipleSwaps()]);

  console.log('=== Deposit Tests completed ===');
}
