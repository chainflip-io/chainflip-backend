import { requestNewSwap } from 'shared/perform_swap';
import { sendBtcTransactionWithMultipleUtxosToSameAddress } from 'shared/send_btc';
import { Asset, newAssetAddress } from 'shared/utils';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { observeEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';

async function testBitcoinMultipleUtxos<A = []>(cf: ChainflipIO<A>) {
  // Configuration for this test:
  const destAsset: Asset = 'ArbEth';

  // The test will send a single bitcoin transaction spending multiple times to our deposit channel.
  // It will use the following amounts for each of the created UTXOs.
  // NOTE: The numbers should be distinct, otherwise we won't be able to ensure that we get `DepositFinalised`
  // events for all of the amounts.
  const fineAmounts: number[] = [50000000, 30000000, 80000000, 90000000];

  // generate new dest address
  const destAddress = await newAssetAddress(destAsset);

  // request deposit channel
  const swapParams = await requestNewSwap(cf, 'Btc', destAsset, destAddress);

  // construct btc tx with multiple outputs to the deposit address
  const txid = await sendBtcTransactionWithMultipleUtxosToSameAddress(
    swapParams.depositAddress,
    fineAmounts,
  );
  cf.debug(`Sending bitcoin tx with multiple outputs: ${txid}`);

  // helper to parse polkadotjs numbers
  const parsePdJsInt = (number: string) => Number(number.replaceAll(',', ''));

  // construct a list of promises waiting for confirmation of deposit of each of the amounts
  const events = fineAmounts.map((fineAmount) =>
    observeEvent(cf.logger, `bitcoinIngressEgress:DepositFinalised`, {
      test: (event) => {
        const amount = parsePdJsInt(event.data.amount);
        cf.debug(`event deposit amount is ${amount}`);
        return (
          event.data.channelId === swapParams.channelId.toString() &&
          amount >= fineAmount * 0.95 &&
          amount <= fineAmount * 1.05
        );
      },
      historicalCheckBlocks: 10,
    }).event.then((e) => {
      cf.debug(`Deposit of ${e.data.amount} finalised for channel ${e.data.channelId}`);
    }),
  );

  // wait for all promises
  await Promise.all(events);
}

export async function testSpecialBitcoinSwaps(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await testBitcoinMultipleUtxos(cf);
}
