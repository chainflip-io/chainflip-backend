import { assetDecimals } from '@chainflip-io/cli';
import Web3 from 'web3';
import { amountToFineAmount, defaultAssetAmounts } from '../shared/utils';
import { testSwap } from './swapping';

export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

  // This successfully egresses a tx that consumes 6842971 gas.
  // I don't see an event specific for the gas. I think that might be necessary for the frontend to display it to users (e.g. final gas budget in tokens after swap).
  // From here, make a test that makes sure we underpay for the gas and we do observeBadEventEVM (aka making sure it's not broadcasted). I guess in the future we
  // can also observe a BroadcastAborted but it will be tricky to track the swap and we can't bundle them in the rest of concurrent swaps. For now I'd observe it
  // as a bad Event if it's emited. In the future we can make more tight tests with valid and invalid gas amounts.
  await testSwap('DOT', 'FLIP', undefined, {
    message: web3.eth.abi.encodeParameters(['string'], ['GasTest']),
    gasBudget: Math.floor(
      Number(amountToFineAmount(defaultAssetAmounts('DOT'), assetDecimals.DOT)) / 100,
    ),
    cfParameters: '0x',
  });
}
