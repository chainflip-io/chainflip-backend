/* eslint-disable @typescript-eslint/lines-between-class-members */
/* eslint-disable @typescript-eslint/no-empty-function */
import { setTimeout as sleep } from 'timers/promises';
import WebSocket, { OPEN } from 'ws';
import { Assets } from '../../enums';
import BrokerClient from '../broker';

jest.mock(
  'ws',
  () =>
    class {
      on() {}
      once() {}
      send() {}
      close() {}
      removeListener() {}
      readyState = OPEN;
    },
);

describe(BrokerClient.prototype.requestSwapDepositAddress, () => {
  let client: BrokerClient;
  const onSpy = jest.spyOn(WebSocket.prototype, 'on');
  const sendSpy = jest.spyOn(WebSocket.prototype, 'send');

  beforeEach(async () => {
    client = await BrokerClient.create();
  });

  afterEach(async () => {
    await client.close();
  });

  it('gets a response from the broker', async () => {
    const resultPromise = client.requestSwapDepositAddress({
      srcAsset: Assets.FLIP,
      destAsset: Assets.USDC,
      srcChain: 'Ethereum',
      destAddress: '0xcafebabe',
      destChain: 'Ethereum',
    });

    // event loop tick to allow promise within client to resolve
    await sleep(0);

    const messageHandler = onSpy.mock.calls[0][1] as (...args: any) => any;

    const requestObject = JSON.parse(sendSpy.mock.calls[0][0] as string);

    expect(requestObject).toStrictEqual({
      id: 0,
      jsonrpc: '2.0',
      method: 'broker_requestSwapDepositAddress',
      params: ['Flip', 'Usdc', '0xcafebabe', 0],
    });

    messageHandler(
      JSON.stringify({
        id: 0,
        jsonrpc: '2.0',
        result: {
          address: '0x1234567890',
          expiry_block: 100,
          issued_block: 50,
          channel_id: 200,
        },
      }),
    );

    await expect(resultPromise).resolves.toStrictEqual({
      address: '0x1234567890',
      expiryBlock: 100,
      issuedBlock: 50,
      channelId: 200n,
    });
  });

  it('submits ccm data', async () => {
    const resultPromise = client.requestSwapDepositAddress({
      srcAsset: Assets.FLIP,
      destAsset: Assets.USDC,
      srcChain: 'Ethereum',
      destAddress: '0xcafebabe',
      destChain: 'Ethereum',
      ccmMetadata: {
        gasBudget: 123,
        message: 'ByteString',
        cfParameters: 'ByteString',
      },
    });

    // event loop tick to allow promise within client to resolve
    await sleep(0);
    const requestObject = JSON.parse(sendSpy.mock.calls[0][0] as string);
    const messageHandler = onSpy.mock.calls[0][1] as (...args: any) => any;
    messageHandler(
      JSON.stringify({
        id: 0,
        jsonrpc: '2.0',
        result: {
          address: '0x1234567890',
          expiry_block: 100,
          issued_block: 50,
          channel_id: 200,
        },
      }),
    );

    expect(requestObject).toStrictEqual({
      id: 0,
      jsonrpc: '2.0',
      method: 'broker_requestSwapDepositAddress',
      params: [
        'Flip',
        'Usdc',
        '0xcafebabe',
        0,
        {
          gas_budget: 123,
          message: 'ByteString',
          cf_parameters: 'ByteString',
          source_chain: 'Ethereum',
          source_address: '0x8ba1f109551bd432803012645ac136ddd64dba72',
        },
      ],
    });
    await expect(resultPromise).resolves.toStrictEqual({
      address: '0x1234567890',
      expiryBlock: 100,
      issuedBlock: 50,
      channelId: 200n,
    });
  });
});
