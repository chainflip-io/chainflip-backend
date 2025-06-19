import { Logger, throwError } from 'shared/utils/logger';

// export const brokerEndpoint = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:10997';
export const brokerApiEndpoint = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:9944';

// export const lpApiEndpoint = 'http://127.0.0.1:10589';
export const lpApiEndpoint = process.env.LP_ENDPOINT || 'http://127.0.0.1:9944';

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
  return jsonRpc(logger, method, params, lpApiEndpoint);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function brokerApiRpc(logger: Logger, method: string, params: any[]): Promise<any> {
  return jsonRpc(logger, method, params, brokerApiEndpoint);
}
