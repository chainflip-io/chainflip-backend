
import Web3 from 'web3';
import { sha256 } from '../shared/utils';

export function newEthAddress(seed: string): string {
    const secret = sha256(seed).toString('hex');
    const web3 = new Web3();
    return web3.eth.accounts.privateKeyToAccount(secret).address;
}