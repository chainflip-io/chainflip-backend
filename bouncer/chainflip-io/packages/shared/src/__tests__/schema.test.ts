import { openSwapDepositChannelSchema } from '../schemas';

const swapBody = {
  srcAsset: 'BTC',
  srcChain: 'Bitcoin',
  destAsset: 'ETH',
  destChain: 'Ethereum',
  destAddress: '0x123',
  amount: '123',
};

describe('postSwapSchema', () => {
  it('handles empty ccmMetadata strings', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
      }),
    ).toEqual(expect.objectContaining({ success: true }));
  });
  it('handles full ccmMetadata', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
        ccmMetadata: {
          gasBudget: '123',
          message: '0xdeadc0de',
          cfParameters: 'string',
        },
      }),
    ).toEqual(expect.objectContaining({ success: true }));
  });
  it('handles without cf parameters', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
        ccmMetadata: {
          gasBudget: '123',
          message: '0xdeadc0de',
        },
      }),
    ).toEqual(expect.objectContaining({ success: true }));
  });
  it('handles missing ccm params', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
        ccmMetadata: {
          gasBudget: '123',
        },
      }),
    ).toEqual(expect.objectContaining({ success: false }));
  });
  it('handles missing ccm params', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
        ccmMetadata: {
          message: '0xdeadc0de',
          cfParameters: 'string',
        },
      }),
    ).toEqual(expect.objectContaining({ success: false }));
  });
  it('handles missing swap body params', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        srcAsset: 'BTC',
        destAsset: 'ETH',
        destAddress: '0x123',
        ccmMetadata: {
          gasBudget: '123',
          message: '0xdeadc0de',
          cfParameters: 'string',
        },
      }),
    ).toEqual(expect.objectContaining({ success: false }));
  });
  it('handles wrong type for gasBudget', () => {
    expect(
      openSwapDepositChannelSchema.safeParse({
        ...swapBody,
        ccmMetadata: {
          gasBudget: '0x123',
          message: '0xdeadc0de',
          cfParameters: 'string',
        },
      }),
    ).toEqual(expect.objectContaining({ success: false }));
  });
});
