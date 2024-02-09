import cli from '../cli';
import cliRequestSwapDepositAddress from '../commands/cliRequestSwapDepositAddress';

jest.mock('../commands/cliRequestSwapDepositAddress', () => ({
  __esModule: true,
  ...jest.requireActual('../commands/cliRequestSwapDepositAddress'),
  default: jest.fn(),
}));

const cmd = `request-swap-deposit-address
--src-asset FLIP
--dest-asset USDC
--dest-address 0x0
--broker-url ws://example.com
--src-chain Ethereum
--dest-chain Ethereum`;

describe('cli', () => {
  it('calls the correct handler with the proper arguments', async () => {
    await cli(cmd.split(/\s+/));
    expect(jest.mocked(cliRequestSwapDepositAddress).mock.lastCall)
      .toMatchInlineSnapshot(`
      [
        {
          "$0": "chainflip-cli",
          "_": [
            "request-swap-deposit-address",
          ],
          "broker-url": "ws://example.com",
          "brokerUrl": "ws://example.com",
          "dest-address": "0x0",
          "dest-asset": "USDC",
          "dest-chain": "Ethereum",
          "destAddress": "0x0",
          "destAsset": "USDC",
          "destChain": "Ethereum",
          "src-asset": "FLIP",
          "src-chain": "Ethereum",
          "srcAsset": "FLIP",
          "srcChain": "Ethereum",
        },
      ]
    `);
  });
});
