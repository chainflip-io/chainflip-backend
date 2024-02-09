import { fundStateChainAccount } from '@/shared/stateChainGateway';
import cli from '../cli';

jest.mock('ethers');
jest.mock('@/shared/stateChainGateway', () => ({
  fundStateChainAccount: jest
    .fn()
    .mockResolvedValue({ status: 1, transactionHash: 'example-tx-hash' }),
}));

const localnet = `fund-state-chain-account
--src-account-id 0x1
--chainflip-network localnet
--amount 1000000
--wallet-private-key 0x2
--state-chain-manager-contract-address 0x3
--flip-token-contract-address 0x4
--eth-network test`;

describe('cli', () => {
  it.each([localnet, localnet.replace('localnet', 'sisyphos')])(
    'calls the correct handler with the proper arguments',
    async (args) => {
      const logSpy = jest.spyOn(global.console, 'log').mockImplementation();
      await cli(args.split(/\s+/));

      expect(fundStateChainAccount).toHaveBeenCalledTimes(1);
      expect(jest.mocked(fundStateChainAccount).mock.calls).toMatchSnapshot();
      expect(logSpy).toHaveBeenCalledWith(
        expect.stringContaining('Transaction hash: example-tx-hash'),
      );
    },
  );
});
