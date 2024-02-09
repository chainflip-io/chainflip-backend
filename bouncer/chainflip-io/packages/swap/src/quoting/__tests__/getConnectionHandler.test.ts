import getConnectionHandler from '../getConnectionHandler';

describe(getConnectionHandler, () => {
  it('ignores malformed quote responses', () => {
    const { handler, quotes$ } = getConnectionHandler();
    const socket = { id: 'socket-id', on: jest.fn() };
    const next = jest.fn();
    quotes$.subscribe(next);

    handler(socket as any);

    const callback = socket.on.mock.calls[1][1];

    callback({ id: 'string', egress_amount: 1 });
    callback({ id: 'string', egress_amount: '2' });

    expect(next).toHaveBeenCalledTimes(1);
  });
});
