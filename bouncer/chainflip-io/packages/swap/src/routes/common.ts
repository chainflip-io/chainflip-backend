import { RequestHandler } from 'express';
import type { RouteParameters, Response } from 'express-serve-static-core';
import type { ParsedQs } from 'qs';
import logger from '../utils/logger';
import ServiceError from '../utils/ServiceError';

export const handleError = (res: Response, error: unknown) => {
  logger.customInfo('received error', {}, { error });

  if (error instanceof ServiceError) {
    res.status(error.code).json({ message: error.message });
  } else {
    logger.customError(
      'unknown error occurred',
      { alertCode: 'UnknownError' },
      { error },
    );
    res.status(500).json({ message: 'unknown error' });
  }
};

/* eslint-disable @typescript-eslint/no-explicit-any */
export const asyncHandler = <
  Route extends string,
  P = RouteParameters<Route>,
  ResBody = any,
  ReqBody = any,
  ReqQuery = ParsedQs,
  Locals extends Record<string, any> = Record<string, any>,
>(
  handler: RequestHandler<P, ResBody, ReqBody, ReqQuery, Locals>,
): typeof handler =>
  (async (req, res, next) => {
    try {
      await handler(req, res, next);
    } catch (error) {
      handleError(res, error);
    }
  }) as RequestHandler<P, ResBody, ReqBody, ReqQuery, Locals>;
/* eslint-enable @typescript-eslint/no-explicit-any */
