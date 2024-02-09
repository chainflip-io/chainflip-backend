import { once } from 'events';
import { AddressInfo, WebSocketServer } from 'ws';
import { z } from 'zod';
import RpcClient from '../RpcClient';

const requestMap = {
  echo: z.tuple([z.string()]),
};

const responseMap = {
  echo: z.string(),
};

describe(RpcClient, () => {
  let serverClosed = false;
  let server: WebSocketServer;

  const closeServer = () => {
    server.close();
    serverClosed = true;
  };

  let client: RpcClient<typeof requestMap, typeof responseMap>;
  const killConnections = () => {
    server.clients.forEach((c) => {
      c.terminate();
    });
  };

  beforeEach(async () => {
    serverClosed = false;
    server = new WebSocketServer({ port: 0, host: '127.0.0.1' });

    server.on('connection', (ws) => {
      ws.on('message', (data) => {
        const rpcRequest = JSON.parse(data.toString());

        ws.send(
          JSON.stringify({
            id: rpcRequest.id,
            jsonrpc: '2.0',
            result: rpcRequest.params[0],
          }),
        );
      });
    });

    await once(server, 'listening');
    const address = server.address() as AddressInfo;
    client = await new RpcClient(
      `ws://127.0.0.1:${address.port}`,
      requestMap,
      responseMap,
      'test',
    ).connect();
  });

  afterEach(async () => {
    await client.close();
    server.close();
    if (!serverClosed) await once(server, 'close');
  });

  it('resends messages if a disconnection happens while awaiting a response', async () => {
    const response = await client.sendRequest('echo', 'hello');

    expect(response).toEqual('hello');

    killConnections();

    const response2 = await client.sendRequest('echo', 'hello');
    expect(response2).toEqual('hello');
    expect(client.eventNames()).toStrictEqual([]);
  });

  it("doesn't spam the reconnect", async () => {
    jest.useFakeTimers();
    const response = await client.sendRequest('echo', 'hello');
    expect(response).toEqual('hello');

    killConnections();
    closeServer();
    await once(client, 'DISCONNECT');
    const connectSpy = jest.spyOn(client, 'connect');

    for (let i = 0; i < 10; i += 1) {
      const promise = once(client, 'DISCONNECT');
      await jest.runOnlyPendingTimersAsync();
      await promise;
      expect(connectSpy).toHaveBeenCalled();
    }

    expect(connectSpy).toHaveBeenCalledTimes(10);
    expect(client.eventNames()).toStrictEqual([]);
  });
});
