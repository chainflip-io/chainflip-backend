type IgnoredField =
  | 'prototype'
  | 'assert'
  | 'captureStackTrace'
  | 'prepareStackTrace'
  | 'stackTraceLimit';

export default class ServiceError extends Error {
  static badRequest(message: string): ServiceError {
    return new ServiceError(message, 400);
  }

  static notFound(message = 'resource not found'): ServiceError {
    return new ServiceError(message, 404);
  }

  static internalError(message = 'internal error'): ServiceError {
    return new ServiceError(message, 500);
  }

  static assert(
    condition: unknown,
    code: Exclude<keyof typeof ServiceError, IgnoredField>,
    message: string,
  ): asserts condition {
    if (!condition) throw ServiceError[code](message);
  }

  constructor(
    message: string,
    readonly code: number,
  ) {
    super(message);

    Error.captureStackTrace(this, ServiceError);
  }
}
