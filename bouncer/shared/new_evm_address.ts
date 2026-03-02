import { getWeb3, sha256 } from 'shared/utils';

export function newEvmAddress(seed: string): string {
  const secret = sha256(seed).toString('hex');
  const web3 = getWeb3('Ethereum'); // any evm client works
  return web3.eth.accounts.privateKeyToAccount(secret).address;
}
