// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Flip(ERC20) balance of the address provided as the first argument.
//
// For example: pnpm tsx ./commands/get_flip_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import { runWithTimeout } from '../shared/utils';
import { getErc20Balance } from '../shared/get_erc20_balance';
import { getEthContractAddress } from '../shared/utils';

async function getFlipBalanceCommand(ethereumAddress: string) {
  const contractAddress = getEthContractAddress('FLIP');
  const balance = await getErc20Balance(ethereumAddress, contractAddress);
  console.log(balance);
  process.exit(0);
}

const ethereumAddress = process.argv[2] ?? '0';

runWithTimeout(getFlipBalanceCommand(ethereumAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
