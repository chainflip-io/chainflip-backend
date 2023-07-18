import { performSwapViaContract, approveTokenVault } from '../shared/contract_swap';
import { runWithTimeout } from '../shared/utils';

async function testAllContractSwaps() {
  await approveTokenVault('USDC', (500000000 * 3).toString());

  await Promise.all([
    performSwapViaContract('ETH', 'DOT'),
    performSwapViaContract('ETH', 'USDC'),
    performSwapViaContract('ETH', 'BTC'),
    performSwapViaContract('USDC', 'DOT'),
    performSwapViaContract('USDC', 'ETH'),
    performSwapViaContract('USDC', 'BTC'),
  ]);
}

// A successful execution usually takes ~150 seconds
runWithTimeout(testAllContractSwaps(), 180000)
  .then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
