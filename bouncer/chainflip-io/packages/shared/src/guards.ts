import { Assets } from './enums';
import { ExecuteSwapParams } from './vault';
import {
  ExecuteCallParams,
  TokenCallParams,
  TokenSwapParams,
} from './vault/schemas';

export const isString = (value: unknown): value is string =>
  typeof value === 'string';

export const isNotNull = <T>(value: T | null): value is T => value !== null;

export const isNotNullish = <T>(value: T | null | undefined): value is T =>
  value !== null && value !== undefined;

export function assert(condition: unknown, message: string): asserts condition {
  if (condition) return;
  const error = new Error(message);

  if (error.stack) {
    // Remove the first line of the stack, which is assert function itself
    error.stack = error.stack.replace(/\n.+/, '\n');
  }

  throw error;
}

export const isTokenSwap = (
  params: ExecuteSwapParams,
): params is TokenSwapParams => params.srcAsset !== Assets.ETH;

export const isTokenCall = (
  params: ExecuteCallParams,
): params is TokenCallParams => params.srcAsset !== Assets.ETH;
