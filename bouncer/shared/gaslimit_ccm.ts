import Web3 from 'web3';
import { Asset } from '@chainflip-io/cli/.';
import { newCcmMetadata, prepareSwap, testSwap } from './swapping';
import { getChainflipApi, observeCcmReceived, observeEvent, observeSwapScheduled } from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { BtcAddressType } from './new_btc_address';

// Currently, gasLimit for CCM calls is hardcoded to 400k. Default gas overhead ~115k. Extra margin of
// 215k (CFTester loop step). Should be broadcasted up to ~220k according to contract tests, but the
// parameter values affect that. 210k is a safe bet for a call being broadcasted, 230k won't be.
const maximumGasReceived = 210000;
const tagSuffix = ' CcmGasLimit';

let stopObserving = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToUse: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToUse]),
    1,
  );
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  gasToUse: number,
  addressType?: BtcAddressType,
) {
  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasToUse);

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix,
  );

  const { depositAddress, channelId } = await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
  );

  const swapScheduledHandle = observeSwapScheduled(sourceAsset, channelId);

  const ccmEventEmitted = observeCcmReceived(
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    () => stopObserving,
  ).then((event) => {
    if (event)
      throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
  });

  await Promise.all([send(sourceAsset, depositAddress), swapScheduledHandle, ccmEventEmitted]);
}

export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  const gasLimitSwapsNotBroadcasted = [
    testGasLimitSwap('DOT', 'FLIP', maximumGasReceived + 20000),
    testGasLimitSwap('ETH', 'USDC', maximumGasReceived + 20000),
    testGasLimitSwap('FLIP', 'ETH', maximumGasReceived + 20000),
  ];
  const gasLimitSwapsBroadcasted = [
    testSwap('DOT', 'FLIP', undefined, gasTestCcmMetadata('DOT', maximumGasReceived), tagSuffix),
    testSwap('ETH', 'USDC', undefined, gasTestCcmMetadata('ETH', maximumGasReceived), tagSuffix),
    testSwap('FLIP', 'ETH', undefined, gasTestCcmMetadata('FLIP', maximumGasReceived), tagSuffix),
  ];

  let broadcastAborted = 0;
  await observeEvent(
    'ethereumBroadcaster:BroadcastAborted',
    await getChainflipApi(),
    (_) => ++broadcastAborted === gasLimitSwapsNotBroadcasted.length,
  );

  stopObserving = true;

  await Promise.all([gasLimitSwapsNotBroadcasted, gasLimitSwapsBroadcasted]);

  console.log('=== GasLimit CCM test completed ===');
}
