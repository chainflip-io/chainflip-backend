let id = 0;
export async function jsonRpc(
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any[],
  endpoint?: string,
): Promise<JSON> {
  console.log('Sending json RPC', method);

  id++;
  const request = JSON.stringify({
    jsonrpc: '2.0',
    method,
    params,
    id,
  });

  const fetchEndpoint = endpoint ?? 'http://127.0.0.1:9944';
  const response = await fetch(`${fetchEndpoint}`, {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
    },
    body: request,
  });

  const data = await response.json();
  if (data.error) {
    throw new Error(`JSON Rpc request ${request} failed: ${data.error.message}`);
  } else {
    return data.result;
  }
}
