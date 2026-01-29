import { PublicKey } from '@solana/web3.js';
import { getAccount, getAssociatedTokenAddressSync } from '@solana/spl-token';
import {
  assetDecimals,
  fineAmountToAmount,
  getContractAddress,
  getEncodedSolAddress,
  getSolConnection,
} from 'shared/utils';

export async function getSolUsdtBalance(address: string): Promise<string> {
  const connection = getSolConnection();
  const usdtMintPubKey = new PublicKey(getContractAddress('Solana', 'SolUsdt'));

  const encodedSolAddress = new PublicKey(getEncodedSolAddress(address));
  const ata = getAssociatedTokenAddressSync(usdtMintPubKey, encodedSolAddress, true);

  const accountInfo = await connection.getAccountInfo(ata);
  const usdtFineAmount = accountInfo ? (await getAccount(connection, ata)).amount : '0';
  return fineAmountToAmount(usdtFineAmount.toString(), assetDecimals('SolUsdt'));
}
