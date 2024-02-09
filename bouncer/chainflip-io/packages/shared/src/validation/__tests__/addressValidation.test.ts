import { Assets, Chains } from '@/shared/enums';
import {
  validateBitcoinMainnetAddress,
  validateBitcoinTestnetAddress,
  validateBitcoinRegtestAddress,
  validatePolkadotAddress,
  validateAddress,
} from '../addressValidation';
import bitcoinAddresses from './bitcoinAddresses.json' assert { type: 'json' };

describe(validatePolkadotAddress, () => {
  it('validates valid addresses', () => {
    expect(
      validatePolkadotAddress(
        '1exaAg2VJRQbyUBAeXcktChCAqjVP9TUxF3zo23R2T6EGdE',
      ),
    ).toBe(true);
  });

  it('rejects invalid addresses', () => {
    expect(
      validatePolkadotAddress(
        '1exaAg2VJRQbyUBAeXcktChCAqjVP9TUxF3zo23R2T6EGde',
      ),
    ).toBe(false);
  });
});

describe(validateBitcoinMainnetAddress, () => {
  it.each(
    Object.entries(bitcoinAddresses).flatMap(([network, addressMap]) =>
      Object.values(addressMap).flatMap((addresses) =>
        addresses.map((address) => [address, network === 'mainnet'] as const),
      ),
    ),
  )('validates valid addresses', (address, expected) => {
    expect(validateBitcoinMainnetAddress(address)).toBe(expected);
  });
});

describe(validateBitcoinTestnetAddress, () => {
  it.each(
    Object.entries(bitcoinAddresses).flatMap(([network, addressMap]) =>
      Object.values(addressMap).flatMap((addresses) =>
        addresses.map((address) => [address, network === 'testnet'] as const),
      ),
    ),
  )('validates valid addresses', (address, expected) => {
    expect(validateBitcoinTestnetAddress(address)).toBe(expected);
  });
});

describe(validateBitcoinRegtestAddress, () => {
  it.each(
    Object.entries(bitcoinAddresses).flatMap(([network, addressMap]) =>
      Object.entries(addressMap).flatMap(([type, addresses]) =>
        addresses.map(
          (address) =>
            [
              address,
              network === 'regtest' ||
                (network === 'testnet' && type !== 'SEGWIT'),
            ] as const,
        ),
      ),
    ),
  )('validates valid addresses', (address, expected) => {
    expect(validateBitcoinRegtestAddress(address)).toBe(expected);
  });
});

describe(validateAddress, () => {
  it.each([
    [Assets.DOT, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    ['DOT', '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.ETH, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Assets.USDC, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Assets.FLIP, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
  ] as const)('returns true for valid supportedAssets %s', (asset, address) => {
    expect(validateAddress(asset, address)).toBeTruthy();
  });

  it.each([
    [Chains.Polkadot, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    ['Polkadot', '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Chains.Ethereum, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    ['Ethereum', '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Chains.Ethereum, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Chains.Ethereum, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
  ] as const)('returns true for valid supportedAssets %s', (asset, address) => {
    expect(validateAddress(asset, address)).toBeTruthy();
  });

  it.each([
    [Chains.Bitcoin, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    ['Bitcoin', '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.BTC, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    ['BTC', '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
  ] as const)(
    'returns false for invalid bitcoin addresses %s',
    (asset, address) => {
      expect(validateAddress(asset, address)).toBeFalsy();
    },
  );

  it.each([
    [Chains.Bitcoin, 'mkPuLFihuytSjmdqLztCXXESD7vrjnTiTP'],
    ['BTC', 'miEfvT7iYiEJxS69uq9MMeBfcLpKjDMpWS'],
  ] as const)(
    'returns true for valid testnet bitcoin addresses %s',
    (asset, address) => {
      expect(validateAddress(asset, address, false)).toBeTruthy();
    },
  );

  it.each([
    [Assets.DOT, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Assets.ETH, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.USDC, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.FLIP, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
  ] as const)('returns false for invalid address %s', (asset, address) => {
    expect(validateAddress(asset, address)).toBeFalsy();
  });
});
