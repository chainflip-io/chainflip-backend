import { Logger, throwError } from './utils/logger';

let id = 0;
export async function jsonRpc(
  logger: Logger,
  method: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  params: any[],
  endpoint?: string,
): Promise<JSON> {
  logger.debug('Sending json RPC', method);

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
    throwError(logger, new Error(`JSON Rpc request ${request} failed: ${data.error.message}`));
  }
  return data.result;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function lpApiRpc(logger: Logger, method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `chainflip-lp-api.service`
  return jsonRpc(logger, method, params, 'http://127.0.0.1:10589');
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function brokerApiRpc(logger: Logger, method: string, params: any[]): Promise<any> {
  return jsonRpc(logger, method, params, 'http://127.0.0.1:10997');
}
