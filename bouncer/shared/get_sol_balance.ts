import { PublicKey } from '@solana/web3.js';
import { assetDecimals, fineAmountToAmount, getEncodedSolAddress, getSolConnection } from './utils';

export async function getSolBalance(address: string): Promise<string> {
  const connection = getSolConnection();

  const lamports = await connection.getBalance(new PublicKey(getEncodedSolAddress(address)));
  return fineAmountToAmount(lamports.toString(), assetDecimals("SOL"));
}
