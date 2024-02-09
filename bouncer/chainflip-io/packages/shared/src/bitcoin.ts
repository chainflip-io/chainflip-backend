import { BitcoinAddress } from 'bech32-buffer';
import { ChainflipNetwork } from './enums';

export const isValidSegwitAddress = (address: string) => {
  const hrp = /^(bc|tb|bcrt)1/.exec(address)?.[1];
  if (!hrp) return false;

  return BitcoinAddress.decode(address).prefix === hrp;
};

const prefixMap = {
  sisyphos: 'tb',
  perseverance: 'tb',
  backspin: 'bcrt',
  mainnet: 'bc',
} as const satisfies Record<ChainflipNetwork, string>;

export const encodeAddress = (
  address: `0x${string}`,
  network: ChainflipNetwork,
) =>
  new BitcoinAddress(
    prefixMap[network],
    1,
    Buffer.from(address.slice(2), 'hex'),
  ).encode();
