import { executeCall } from '@/shared/vault';
import cli from '../cli';

jest.mock('ethers');
jest.mock('@/shared/vault', () => ({
  executeCall: jest
    .fn()
    .mockResolvedValue({ status: 1, transactionHash: 'example-tx-hash' }),
}));

const localnet = `call
  --wallet-private-key 0x2
  --chainflip-network localnet
  --dest-asset USDC
  --amount 1000000000
  --dest-address 0x0
  --message=0xdeadc0de
  --gasAmount=500000
  --src-token-contract-address 0x0
  --vault-contract-address 0x0
  --eth-network test`;

describe('cli', () => {
  it.each([localnet, localnet.replace('localnet', 'sisyphos')])(
    'calls the correct handler with the proper arguments',
    async (args) => {
      const logSpy = jest.spyOn(global.console, 'log').mockImplementation();
      await cli(args.split(/\s+/));

      expect(executeCall).toHaveBeenCalledTimes(1);
      expect(jest.mocked(executeCall).mock.calls).toMatchSnapshot();
      expect(logSpy).toHaveBeenCalledWith(
        expect.stringContaining('Transaction hash: example-tx-hash'),
      );
    },
  );
});
