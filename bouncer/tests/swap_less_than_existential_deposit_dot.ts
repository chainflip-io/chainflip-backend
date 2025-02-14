import { getDotBalance } from '../shared/get_dot_balance';
import { performAndTrackSwap } from '../shared/perform_swap';
import { TestContext } from '../shared/utils/test_context';
import { getSwapRate, newAddress } from '../shared/utils';

const DOT_EXISTENTIAL_DEPOSIT = 1;

export async function swapLessThanED(textContext: TestContext) {
  const tag = `Usdc -> Dot`;
  const logger = textContext.logger.child({ tag });

  // we will try to swap with 5 Usdc and check if the expected output is low enough
  // otherwise we'll keep reducing the amount
  let retry = true;
  let inputAmount = '5';
  while (retry) {
    let outputAmount = await getSwapRate('Usdc', 'Dot', inputAmount);

    while (parseFloat(outputAmount) >= DOT_EXISTENTIAL_DEPOSIT) {
      inputAmount = (parseFloat(inputAmount) / 2).toString();
      outputAmount = await getSwapRate('Usdc', 'Dot', inputAmount);
    }
    logger.debug(`Input amount: ${inputAmount} Usdc`);
    logger.debug(`Approximate expected output amount: ${outputAmount} Dot`);

    // we want to be sure to have an address with 0 balance, hence we create a new one every time
    const address = await newAddress(
      'Dot',
      '!testing less than ED output for dot swaps!' + inputAmount + outputAmount,
    );
    logger.debug(`Generated Dot address: ${address}`);

    await performAndTrackSwap(logger, 'Usdc', 'Dot', address, inputAmount, tag);
    // if for some reason the balance after swapping is > 0 it means that the output was larger than
    // ED, so we'll retry the test with a lower input
    if (parseFloat(await getDotBalance(address)) > 0) {
      logger.debug(`Swap output was more than ED, retrying with less...`);
      inputAmount = (parseFloat(inputAmount) / 3).toString();
    } else {
      retry = false;
    }
  }
}
