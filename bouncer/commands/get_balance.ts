#!/usr/bin/env -S pnpm tsx
import { getBalance as getBalanceShared } from '../shared/get_balance';
import { parseAssetString } from '../shared/utils';

async function getBalance(ccy: string, address: string) {
  const trimmedAddress = address.trim();
  const result = await getBalanceShared(parseAssetString(ccy), trimmedAddress);
  const resultStr = result.toString().trim();
  console.log(resultStr);
}

const ccy = process.argv[2];
const address = process.argv[3];
getBalance(ccy, address);
