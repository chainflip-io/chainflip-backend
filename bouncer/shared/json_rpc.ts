let id = 0;
export async function jsonRpc(
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any[],
  endpoint?: string,
  retries: number = 0,
): Promise<JSON> {
  console.log('Sending json RPC', method);

  id++;
  const request = JSON.stringify({
    jsonrpc: '2.0',
    method,
    params,
    id,
  });

  let retry = 0;
  const fetchEndpoint = endpoint ?? 'http://127.0.0.1:9944';
  for (;;) {
    const response = await fetch(`${fetchEndpoint}`, {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        'Content-Type': 'application/json',
      },
      body: request,
    });

    const data = await response.json();
    retry++;
    if (data.error) {
      if (retry > retries) {
        throw new Error(`JSON Rpc request ${request} failed: ${data.error.message}`);
      } else {
        console.error(
          `JSON Rpc request ${request} failed: ${data.error.message}. Retrying... ${retry}`,
        );
      }
    } else {
      return data.result;
    }
  }
}
