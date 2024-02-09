/* eslint-disable max-classes-per-file */
import { ContractTransaction, VoidSigner } from 'ethers';
import { ERC20__factory } from '../abis';
import { approve, checkAllowance } from '../contracts';

class MockERC20 {
  async allowance(_owner: string, _spender: string): Promise<bigint> {
    throw new Error('unmocked call');
  }

  async approve(
    _spender: string,
    _amount: bigint,
  ): Promise<ContractTransaction> {
    throw new Error('unmocked call');
  }
}

jest.mock('@/shared/abis/factories/ERC20__factory', () => ({
  ERC20__factory: class {
    static connect() {
      return new MockERC20();
    }
  },
}));
const spender = '0xdeadbeef';

describe(checkAllowance, () => {
  it.each([
    { allowance: 1000n, spend: 100n, expected: true },
    { allowance: 1000n, spend: 1500n, expected: false },
  ])('returns the allowance', async ({ allowance, spend, expected }) => {
    const allowanceSpy = jest
      .spyOn(MockERC20.prototype, 'allowance')
      .mockResolvedValueOnce(BigInt(allowance));
    const signer = new VoidSigner('0xcafebabe');

    const result = await checkAllowance(spend, spender, '0x0', signer);

    expect(result.isAllowable).toBe(expected);
    expect(result.allowance).toEqual(BigInt(allowance));
    expect(allowanceSpy.mock.calls).toMatchSnapshot();
  });
});

describe(approve, () => {
  it.each([
    { allowance: 100n, spend: 1000n },
    { allowance: 0n, spend: 1000n },
  ])(
    'approves the spender for an allowance equal to the spend request',
    async ({ allowance, spend }) => {
      const approveSpy = jest
        .spyOn(MockERC20.prototype, 'approve')
        .mockResolvedValueOnce({
          wait: jest.fn().mockResolvedValue({ status: 1, hash: 'TX_HASH' }),
        } as unknown as ContractTransaction);

      const receipt = await approve(
        spend,
        spender,
        ERC20__factory.connect('0x0', new VoidSigner('0xcafebabe')),
        allowance,
        { nonce: 1 },
      );

      expect(receipt).not.toBe(null);
      expect(approveSpy.mock.calls).toMatchSnapshot();
    },
  );

  it('returns null if the allowance is already sufficient', async () => {
    const receipt = await approve(
      10n,
      spender,
      ERC20__factory.connect('0x0', new VoidSigner('0xcafebabe')),
      1000n,
      { nonce: 1 },
    );

    expect(receipt).toBe(null);
  });
});
