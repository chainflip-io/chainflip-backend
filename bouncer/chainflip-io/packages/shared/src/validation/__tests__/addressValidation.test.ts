import { bitcoinAddresses } from './bitcoinAddresses';
import { Assets, assetChains } from '../../enums';
import {
  validateBitcoinMainnetAddress,
  validateBitcoinTestnetAddress,
  validateBitcoinRegtestAddress,
  validatePolkadotAddress,
  validateAddress,
} from '../addressValidation';

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
    expect(
      validateAddress(assetChains[asset], address, 'mainnet'),
    ).toBeTruthy();
    expect(
      validateAddress(assetChains[asset], address, 'perseverance'),
    ).toBeTruthy();
    expect(
      validateAddress(assetChains[asset], address, 'backspin'),
    ).toBeTruthy();
  });

  it.each([
    [Assets.BTC, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    ['BTC', '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.BTC, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    ['BTC', '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
  ] as const)(
    'returns false for invalid bitcoin addresses %s',
    (asset, address) => {
      expect(
        validateAddress(assetChains[asset], address, 'mainnet'),
      ).toBeFalsy();
      expect(
        validateAddress(assetChains[asset], address, 'perseverance'),
      ).toBeFalsy();
      expect(
        validateAddress(assetChains[asset], address, 'backspin'),
      ).toBeFalsy();
    },
  );

  it.each([
    [Assets.BTC, 'mkPuLFihuytSjmdqLztCXXESD7vrjnTiTP', 'perseverance'],
    ['BTC', 'miEfvT7iYiEJxS69uq9MMeBfcLpKjDMpWS', 'perseverance'],
    [
      Assets.BTC,
      'tb1pk5vhse48d90a5pdpgwpm9aegqv5h2p79hxjtknlqusjnc08yklas8xtf35',
      'perseverance',
    ],
    [Assets.BTC, '2NBtZHa1TSuX7xXej8Z63npiHji3y43znRu', 'sisyphos'],
    [
      Assets.BTC,
      'tb1pk5vhse48d90a5pdpgwpm9aegqv5h2p79hxjtknlqusjnc08yklas8xtf35',
      'sisyphos',
    ],
    [Assets.BTC, 'mx7Kg1cDpiWUm1Ru3ogECsFvzrTWjAWMyE', 'backspin'],
    [
      Assets.BTC,
      'bcrt1p785mga8djc3r5f7afaechlth4laxaty2rt08ncgydw4j7zv8np5suf7etv',
      'backspin',
    ],
    [Assets.BTC, 'bc1qvwmuc3pjhwju287sjs5vg7467t2jlymnmjyatp', 'mainnet'],
    [
      Assets.BTC,
      'bc1p7jc7jx0z32gcm5k3dewpqra2vv303jnnhurhrwl384kgnnhsp73qf9a9c3',
      'mainnet',
    ],
  ] as const)(
    'returns true for valid testnet bitcoin addresses %s',
    (asset, address, network) => {
      expect(
        validateAddress(assetChains[asset], address, network),
      ).toBeTruthy();
    },
  );

  it.each([
    [Assets.DOT, '0x02679b10f7b94fc4f273569cc2e5c49eefa5c0f1'],
    [Assets.ETH, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.USDC, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
    [Assets.FLIP, '13NZffZRSQFdg5gYLJBdj5jVtkeDPqF3czLdJ9m6fyHcMjki'],
  ] as const)('returns false for invalid address %s', (asset, address) => {
    expect(validateAddress(assetChains[asset], address, 'mainnet')).toBeFalsy();
  });
});
