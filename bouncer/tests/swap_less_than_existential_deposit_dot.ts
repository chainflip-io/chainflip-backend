import { getDotBalance } from 'shared/get_dot_balance';
import { performAndTrackSwap } from 'shared/perform_swap';
import { TestContext } from 'shared/utils/test_context';
import { getSwapRate, newAssetAddress } from 'shared/utils';
import { newChainflipIO } from 'shared/utils/chainflip_io';

const DOT_EXISTENTIAL_DEPOSIT = 1;

export async function swapLessThanED(testContext: TestContext) {
  const parentCf = await newChainflipIO(testContext.logger, []);
  const tag = `Usdc -> HubDot`;
  const cf = parentCf.withChildLogger(tag);

  // we will try to swap with 5 Usdc and check if the expected output is low enough
  // otherwise we'll keep reducing the amount
  let retry = true;
  let inputAmount = '5';
  while (retry) {
    let outputAmount = await getSwapRate('Usdc', 'HubDot', inputAmount);

    while (parseFloat(outputAmount) >= DOT_EXISTENTIAL_DEPOSIT) {
      inputAmount = (parseFloat(inputAmount) / 2).toString();
      outputAmount = await getSwapRate('Usdc', 'HubDot', inputAmount);
    }
    cf.debug(`Input amount: ${inputAmount} Usdc`);
    cf.debug(`Approximate expected output amount: ${outputAmount} HubDot`);

    // we want to be sure to have an address with 0 balance, hence we create a new one every time
    const address = await newAssetAddress(
      'HubDot',
      '!testing less than ED output for dot swaps!' + inputAmount + outputAmount,
    );
    cf.debug(`Generated Dot address: ${address}`);

    await performAndTrackSwap(parentCf, 'Usdc', 'HubDot', address, inputAmount);
    // if for some reason the balance after swapping is > 0 it means that the output was larger than
    // ED, so we'll retry the test with a lower input
    if (parseFloat(await getDotBalance(address)) > 0) {
      cf.debug(`Swap output was more than ED, retrying with less...`);
      inputAmount = (parseFloat(inputAmount) / 3).toString();
    } else {
      retry = false;
    }
  }
}
