import Web3 from 'web3';
import { Asset } from '@chainflip-io/cli/.';
import { newCcmMetadata, prepareSwap } from './swapping';
import { getChainflipApi, observeCcmReceived, observeEvent, observeSwapScheduled } from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { BtcAddressType } from './new_btc_address';

let stopObserving = false;

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  const messageMetadata = newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string'], ['GasTest']),
    1,
  );

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    ' CcmGasLimit',
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

  const gasLimitTests = [
    testGasLimitSwap('DOT', 'FLIP'),
    testGasLimitSwap('ETH', 'USDC'),
    testGasLimitSwap('FLIP', 'ETH'),
  ];

  let broadcastAborted = 0;
  await observeEvent(
    'ethereumBroadcaster:BroadcastAborted',
    await getChainflipApi(),
    (_) => ++broadcastAborted === gasLimitTests.length,
  );

  stopObserving = true;

  await Promise.all(gasLimitTests);

  console.log('=== GasLimit CCM test completed ===');
}
