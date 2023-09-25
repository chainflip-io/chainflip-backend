let id = 0;
export async function jsonRpc(
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any[],
  port: number,
): Promise<JSON> {
  console.log('Sending json RPC', method);

  id++;
  const response = await fetch(`http://127.0.0.1:${port}`, {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      jsonrpc: '2.0',
      method,
      params,
      id,
    }),
  });

  const data = await response.json();
  if (data.error) {
    throw new Error(`${method} failed: ${data.error.message}`);
  } else {
    return data.result;
  }
}
