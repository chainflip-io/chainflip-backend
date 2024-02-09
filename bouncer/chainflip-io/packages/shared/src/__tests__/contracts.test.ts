/* eslint-disable max-classes-per-file */
import { BigNumberish, ContractTransaction, VoidSigner, ethers } from 'ethers';
import { ERC20__factory } from '../abis';
import { GOERLI_USDC_CONTRACT_ADDRESS } from '../consts';
import { approve, checkAllowance } from '../contracts';

class MockERC20 {
  async allowance(_owner: string, _spender: string): Promise<ethers.BigNumber> {
    throw new Error('unmocked call');
  }

  async approve(
    _spender: string,
    _amount: BigNumberish,
  ): Promise<ContractTransaction> {
    throw new Error('unmocked call');
  }
}

jest.mock('@/shared/abis/factories/ERC20__factory', () => ({
  ERC20__factory: class {
    static connect: () => MockERC20 = jest.fn(() => new MockERC20());
  },
}));
const spender = '0xdeadbeef';

describe(checkAllowance, () => {
  it.each([
    { allowance: 1000, spend: 100, expected: true },
    { allowance: 1000, spend: 1500, expected: false },
  ])('returns the allowance', async ({ allowance, spend, expected }) => {
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(ethers.BigNumber.from(allowance));
    const signer = new VoidSigner('0xcafebabe');

    const result = await checkAllowance(
      spend,
      spender,
      GOERLI_USDC_CONTRACT_ADDRESS,
      signer,
    );

    expect(result.isAllowable).toBe(expected);
    expect(result.allowance).toEqual(ethers.BigNumber.from(allowance));
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
  });
});

describe(approve, () => {
  it.each([
    { allowance: 100, spend: 1000 },
    { allowance: 0, spend: 1000 },
  ])(
    'approves the spender for an allowance equal to the spend request',
    async ({ allowance, spend }) => {
      const approveSpy = jest
        .spyOn(MockERC20.prototype, 'approve')
        .mockResolvedValueOnce({
          wait: jest
            .fn()
            .mockResolvedValue({ status: 1, transactionHash: 'TX_HASH' }),
        });

      const receipt = await approve(
        spend,
        spender,
        ERC20__factory.connect(
          GOERLI_USDC_CONTRACT_ADDRESS,
          new VoidSigner('0xcafebabe'),
        ),
        allowance,
        1,
      );

      expect(receipt).not.toBe(null);
      expect(approveSpy.mock.calls).toMatchSnapshot();
    },
  );

  it('returns null if the allowance is already sufficient', async () => {
    const receipt = await approve(
      10,
      spender,
      ERC20__factory.connect(
        GOERLI_USDC_CONTRACT_ADDRESS,
        new VoidSigner('0xcafebabe'),
      ),
      1000,
      1,
    );

    expect(receipt).toBe(null);
  });
});
