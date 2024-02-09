import { isHex, u8aToHex } from '@polkadot/util';
import {
  base58Decode,
  decodeAddress,
  encodeAddress,
} from '@polkadot/util-crypto';
import * as ethers from 'ethers';
import { isValidSegwitAddress } from '../bitcoin';
import { Chain, ChainflipNetwork } from '../enums';
import { assert } from '../guards';

export type AddressValidator = (address: string) => boolean;

export const validatePolkadotAddress: AddressValidator = (address) => {
  try {
    const bytes = decodeAddress(address);
    if (isHex(address)) {
      const pubkey = u8aToHex(bytes);
      if (pubkey.length !== 66) return false; // we only support 32 byte dot addresses (from dan)
    }
    encodeAddress(bytes);
    return true;
  } catch {
    return false;
  }
};

export const validateEvmAddress: AddressValidator = (address) =>
  ethers.isAddress(address);

type BitcoinNetwork = 'mainnet' | 'testnet' | 'regtest';

const assertArraylikeEqual = <T>(a: ArrayLike<T>, b: ArrayLike<T>) => {
  assert(a.length === b.length, 'arraylike lengths must be equal');
  for (let i = 0; i < a.length; i += 1) {
    assert(a[i] === b[i], 'arraylike elements must be equal');
  }
};

const validateP2PKHOrP2SHAddress = (
  address: string,
  network: BitcoinNetwork,
) => {
  try {
    // The address must be a valid base58 encoded string.
    const decoded = base58Decode(address);

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
    const doubleHash = ethers.getBytes(
      ethers.sha256(ethers.sha256(decoded.slice(0, 21))),
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

const validators: Record<ChainflipNetwork, Record<Chain, AddressValidator>> = {
  mainnet: {
    Bitcoin: validateBitcoinMainnetAddress,
    Ethereum: validateEvmAddress,
    Polkadot: validatePolkadotAddress,
  },
  perseverance: {
    Bitcoin: validateBitcoinTestnetAddress,
    Ethereum: validateEvmAddress,
    Polkadot: validatePolkadotAddress,
  },
  sisyphos: {
    Bitcoin: validateBitcoinTestnetAddress,
    Ethereum: validateEvmAddress,
    Polkadot: validatePolkadotAddress,
  },
  backspin: {
    Bitcoin: validateBitcoinRegtestAddress,
    Ethereum: validateEvmAddress,
    Polkadot: validatePolkadotAddress,
  },
};

export const validateAddress = (
  chain: Chain,
  address: string,
  network: ChainflipNetwork,
): boolean => validators[network][chain](address);
