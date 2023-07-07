// INSTRUCTIONS
//
// This command takes two arguments.
// It will send FLIP to the ethereum address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted as FLIP
//
// For example: pnpm tsx ./commands/send_flip.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 5.5
// will send 5.5 FLIP to the account with address 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { sendErc20 } from '../shared/send_erc20';
import { getEthContractAddress } from '../shared/utils';

async function main(): Promise<void> {
  const ethereumAddress = process.argv[2];
  const flipAmount = process.argv[3].trim();

  const contractAddress = getEthContractAddress('FLIP');
  await sendErc20(ethereumAddress, contractAddress, flipAmount);

  process.exit(0);
}

runWithTimeout(main(), 50000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
