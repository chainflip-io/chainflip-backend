import { Asset } from '@chainflip-io/cli/.';
import { getBalance as getBalanceShared } from '../shared/get_balance';

async function getBalance(ccy: string, address: string) {
  const trimmedAddress = address.trim();
  const result = await getBalanceShared(ccy.toUpperCase() as Asset, trimmedAddress);
  const resultStr = result.toString().trim();
  console.log(resultStr);
}

const ccy = process.argv[2];
const address = process.argv[3];
getBalance(ccy, address);
