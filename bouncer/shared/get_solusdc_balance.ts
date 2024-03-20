import { PublicKey } from '@solana/web3.js';
import { getAccount, getAssociatedTokenAddressSync } from '@solana/spl-token';
import {
  assetDecimals,
  fineAmountToAmount,
  getContractAddress,
  getEncodedSolAddress,
  getSolConnection,
} from './utils';

export async function getSolUsdcBalance(address: string): Promise<string> {
  const connection = getSolConnection();
  const usdcMintPubKey = new PublicKey(getContractAddress('Solana', 'SOLUSDC'));

  const encodedSolAddress = new PublicKey(getEncodedSolAddress(address));
  const ata = getAssociatedTokenAddressSync(usdcMintPubKey, encodedSolAddress, true);

  const accountInfo = await connection.getAccountInfo(ata);
  const usdcFineAmount = accountInfo ? (await getAccount(connection, ata)).amount : '0';
  return fineAmountToAmount(usdcFineAmount.toString(), assetDecimals('SOLUSDC'));
}
