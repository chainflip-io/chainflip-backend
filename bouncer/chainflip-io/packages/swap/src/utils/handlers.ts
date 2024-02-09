type Handler<T extends string, U> = {
  name: T;
  handler: U;
};

type NameWithSpec<T extends string> = `${T}-${number}`;

type HandlerMap<T extends string, U> = Record<T | NameWithSpec<T>, U>;

/**
 * It's difficult to fully capture all this simple function does in words. See
 * the test for a visual representation of how this function works. The gist is
 * it takes an array of specs and handlers and creates a map of handlers for
 * each possible spec to ensure O(1) lookup time at runtime
 */
export const buildHandlerMap = <T extends string, U>(
  specs: { spec: number; handlers: Handler<T, U>[] }[],
): HandlerMap<T, U> => {
  const maxSpec = Math.max(...specs.map(({ spec }) => spec));

  const sorted = specs.slice().sort((a, b) => a.spec - b.spec);

  const result = {} as HandlerMap<T, U>;

  for (const { spec, handlers } of sorted) {
    for (const { name, handler } of handlers) {
      for (let i = spec; i <= maxSpec; i += 1) {
        result[`${name}-${i}`] = handler;
        result[name] = handler;
      }
    }
  }

  return result;
};

export const getDispatcher =
  <T extends string, U>(map: HandlerMap<T, U>) =>
  (name: string, specId: string) => {
    // the specId is in the format of "chainflip-node@<specId>"
    const specNumber = specId.split('@')[1];

    const handlerName = `${name}-${specNumber}` as `${T}-${number}`;

    return map[handlerName] ?? map[name as T] ?? null;
  };
