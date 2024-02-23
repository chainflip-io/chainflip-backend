import { assetDecimals } from '@chainflip-io/cli';
import { PublicKey } from '@solana/web3.js';
import { fineAmountToAmount, getSolConnection } from './utils';

export async function getSolBalance(address: string): Promise<string> {
  const connection = getSolConnection();

  const lamports = await connection.getBalance(new PublicKey(address));
  return fineAmountToAmount(lamports.toString(), assetDecimals.SOL);
}
