import { isHex, hexToU8a } from '@polkadot/util';
import { decodeAddress, encodeAddress } from '@polkadot/util-crypto';
import * as ethers from 'ethers';
import { Asset, Assets, Chain, Chains } from '../enums';
import { assert } from '../guards';
import { isValidSegwitAddress } from './segwitAddr';

export type AddressValidator = (address: string) => boolean;

export const validatePolkadotAddress: AddressValidator = (address) => {
  try {
    encodeAddress(isHex(address) ? hexToU8a(address) : decodeAddress(address));
    return true;
  } catch {
    return false;
  }
};

export const validateEvmAddress: AddressValidator = (address) =>
  ethers.utils.isAddress(address);

type BitcoinNetwork = 'mainnet' | 'testnet' | 'regtest';

const assertArraylikeEqual = <T>(a: ArrayLike<T>, b: ArrayLike<T>) => {
  assert(a.length === b.length, 'arraylike lengths must be equal');
  for (let i = 0; i < a.length; i += 1) {
    assert(a[i] === b[i], 'arraylike elements must be equal');
  }
};

// if we go back to ethers 6 when typechain updates
// const hexToUint8Array = (hex: string) => {
//   const withoutPrefix = hex.replace(/^0x/, '');
//   const padded =
//     withoutPrefix.length % 2 === 0 ? withoutPrefix : `0${withoutPrefix}`;
//   const matchArray = padded.match(/.{2}/g);
//   assert(matchArray, 'matchArray must not be null');
//   return new Uint8Array(matchArray.map((n) => Number.parseInt(n, 16)));
// };

// const decodeBase58 = (address: string) => {
//   const bigint = ethers.decodeBase58(address);
//   // this regex will always match
//   // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
//   const leadingZeroes = address.match(/^1*/)![0].length;
//   const hex = `${'0'.repeat(leadingZeroes)}${bigint.toString(16)}`;
//   return hexToUint8Array(hex);
// };

const validateP2PKHOrP2SHAddress = (
  address: string,
  network: BitcoinNetwork,
) => {
  try {
    // The address must be a valid base58 encoded string.
    const decoded = ethers.utils.base58.decode(address);

    // Decoding it must result in exactly 25 bytes.
    assert(decoded.length === 25, 'decoded address must be 25 bytes long');

    if (network === 'mainnet') {
      // On mainnet, the first decoded byte must be "0x00" or "0x05".
      assert(
        decoded[0] === 0x00 || decoded[0] === 0x05,
        'decoded address must start with 0x00 or 0x05',
      );
    } else {
      // On testnet/regtest, the first decoded byte must be "0x6F" or "0xC4".
      assert(
        decoded[0] === 0x6f || decoded[0] === 0xc4,
        'decoded address must start with 0x6f or 0xc4',
      );
    }
    // The last 4 decoded bytes must be equal to the first 4 bytes of the double sha256 of the first 21 decoded bytes
    const checksum = decoded.slice(-4);
    const doubleHash = ethers.utils.arrayify(
      ethers.utils.sha256(ethers.utils.sha256(decoded.slice(0, 21))),
    );

    assertArraylikeEqual(checksum, doubleHash.slice(0, 4));

    return true;
  } catch (error) {
    // console.error(error);
    return false;
  }
};

const validateSegwitAddress = (address: string, network: BitcoinNetwork) => {
  try {
    assert(
      // On mainnet, the address must start with "bc1"
      (network === 'mainnet' && address.startsWith('bc1')) ||
        // on testnet it must start with "tb1"
        (network === 'testnet' && address.startsWith('tb1')) ||
        // on regtest it must start with "bcrt1"
        (network === 'regtest' && address.startsWith('bcrt1')),
      'address must start with bc1, tb1 or bcrt1',
    );

    return isValidSegwitAddress(address);
  } catch {
    return false;
  }
};

const validateBitcoinAddress = (address: string, network: BitcoinNetwork) =>
  validateP2PKHOrP2SHAddress(address, network) ||
  validateSegwitAddress(address, network);

export const validateBitcoinMainnetAddress: AddressValidator = (
  address: string,
) => validateBitcoinAddress(address, 'mainnet');

export const validateBitcoinTestnetAddress: AddressValidator = (
  address: string,
) => validateBitcoinAddress(address, 'testnet');

export const validateBitcoinRegtestAddress: AddressValidator = (
  address: string,
) => validateBitcoinAddress(address, 'regtest');

export const validateChainAddress = (
  address: string,
  isMainnet = true,
): Record<Chain | Asset, boolean> => ({
  [Assets.ETH]: validateEvmAddress(address),
  [Assets.BTC]: isMainnet
    ? validateBitcoinMainnetAddress(address)
    : validateBitcoinTestnetAddress(address) ||
      validateBitcoinRegtestAddress(address),
  [Assets.DOT]: validatePolkadotAddress(address),
  [Assets.FLIP]: validateEvmAddress(address),
  [Assets.USDC]: validateEvmAddress(address),
  [Chains.Ethereum]: validateEvmAddress(address),
  [Chains.Bitcoin]: isMainnet
    ? validateBitcoinMainnetAddress(address)
    : validateBitcoinTestnetAddress(address) ||
      validateBitcoinRegtestAddress(address),
  [Chains.Polkadot]: validatePolkadotAddress(address),
});

export const validateAddress = (
  assetOrChain: Chain | Asset | undefined,
  address: string,
  isMainnet = true,
): boolean => {
  if (!assetOrChain) return validateEvmAddress(address);
  return validateChainAddress(address, isMainnet)[assetOrChain];
};
