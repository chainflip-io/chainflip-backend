import { ChainflipNetwork } from '../enums';
import {
  btcAddress,
  dotAddress,
  hexStringWithMaxByteSize,
  u128,
  unsignedInteger,
} from '../parsers';
import { bitcoinAddresses } from '../validation/__tests__/bitcoinAddresses';

describe('btc parser', () => {
  it.each([
    [Object.values(bitcoinAddresses.mainnet).flat(), 'mainnet', 'sisyphos'],
    [Object.values(bitcoinAddresses.testnet).flat(), 'sisyphos', 'mainnet'],
    [Object.values(bitcoinAddresses.testnet).flat(), 'perseverance', 'mainnet'],
    [Object.values(bitcoinAddresses.regtest).flat(), 'sisyphos', 'mainnet'],
    [Object.values(bitcoinAddresses.regtest).flat(), 'perseverance', 'mainnet'],
    [Object.values(bitcoinAddresses.regtest).flat(), 'backspin', 'mainnet'],
    [Object.values(bitcoinAddresses.regtest).flat(), undefined, 'mainnet'],
  ])(
    'validates btc address %s to be true for the right network',
    (address, network, wrongNetwork) => {
      address.forEach((addr) =>
        expect(
          btcAddress(network as ChainflipNetwork).safeParse(addr).success,
        ).toBeTruthy(),
      );
      address.forEach((addr) =>
        expect(
          btcAddress(wrongNetwork as ChainflipNetwork).safeParse(addr).success,
        ).toBeFalsy(),
      );
    },
  );
  const wrongAddresses = [
    'br1qxy2kgdygjrsqtzq2n0yrf249',
    '',
    '0x71C7656EC7ab88b098defB751B7401B5f6d8976F',
    '5F3sa2TJAWMqDhXG6jhV4N8ko9SxwGy8TpaNS1repo5EYjQX',
  ];
  it.each([
    [wrongAddresses, 'mainnet'],
    [wrongAddresses, 'sisyphos'],
    [wrongAddresses, 'perseverance'],
  ])(`validates btc address %s to be false`, (address, network) => {
    expect(
      btcAddress(network as ChainflipNetwork).safeParse(address).success,
    ).toBeFalsy();
  });
});

describe('dotAddress', () => {
  it('validates dot address and transforms a dot address', async () => {
    expect(dotAddress.parse('0x0')).toBe('1126');
    expect(
      dotAddress.parse(
        '0x9999999999999999999999999999999999999999999999999999999999999999',
      ),
    ).toBe('14UPxPveENj36SF5YX8R2YMrb6HaS7Nuuxw5a1aysuxVZyDu');
  });
});

describe('u128', () => {
  it('handles numbers', () => {
    expect(u128.parse(123)).toBe(123n);
  });

  it('handles numeric strings', () => {
    expect(u128.parse('123')).toBe(123n);
  });

  it('handles hex strings', () => {
    expect(u128.parse('0x123')).toBe(291n);
  });

  it('rejects invalid hex string', () => {
    expect(() => u128.parse('0x123z')).toThrow();
    expect(() => u128.parse('0x')).toThrow();
  });
});

describe('unsignedInteger', () => {
  it('handles numeric strings', () => {
    expect(unsignedInteger.parse('123')).toBe(123n);
  });

  it('handles hex strings', () => {
    expect(unsignedInteger.parse('0x123')).toBe(291n);
  });

  it('handles numbers', () => {
    expect(unsignedInteger.parse(123)).toBe(123n);
  });

  it('rejects invalid hex string', () => {
    expect(() => u128.parse('0x123z')).toThrow();
    expect(() => u128.parse('0x')).toThrow();
  });
});

describe('hexStringWithMaxByteSize', () => {
  it('should accept hex string', () => {
    expect(hexStringWithMaxByteSize(100).parse('0x0123456789abcdef')).toEqual(
      '0x0123456789abcdef',
    );
  });

  it('should accept hex string with exactly max bytes', () => {
    expect(hexStringWithMaxByteSize(3).parse('0xabcdef')).toEqual('0xabcdef');
  });

  it('should reject non hex strings', () => {
    expect(() => hexStringWithMaxByteSize(100).parse('hello')).toThrow(
      'Invalid input',
    );
  });

  it('should reject strings larger than max byte size', () => {
    expect(() => hexStringWithMaxByteSize(2).parse('0xabcdef')).toThrow(
      'String must be less than or equal to 2 bytes',
    );
  });

  it('should accept hex string with different cases', () => {
    expect(hexStringWithMaxByteSize(100).parse('0xAbCdeF')).toEqual('0xAbCdeF');
  });
});
