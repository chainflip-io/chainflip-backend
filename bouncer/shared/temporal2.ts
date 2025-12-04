#!/usr/bin/env -S pnpm tsx

import { z } from 'zod';

type Proposition =
  | {
      __kind: 'then';
      then: <A>(
        cont: <I extends z.ZodTypeAny>(is: I, run: (i: z.infer<I>) => Proposition) => A,
      ) => A;
    }
  | {
      __kind: 'done';
    };

const Done: Proposition = { __kind: 'done' };
function Then<I extends z.ZodTypeAny>(si: I, run: (i: z.infer<I>) => Proposition): Proposition {
  return {
    __kind: 'then',
    then(cont) {
      return cont(si, run);
    },
  };
}

function ex(): Proposition {
  // f(z.number).then((n) =>
  //     f(z.literal(n)).then((n) =>

  //     )
  // )

  // const n = yield Then(z.number);
  // const _ = yield Then(z.literal(n));
  // yield Done;

  return Then(z.number(), (n) => Then(z.literal(n), (_) => Done));
}

function* bla1(): Generator<z.ZodTypeAny> {
  const n = yield z.number();
  const b = yield* AwaitEvent(z.boolean());
  if (b) {
    yield* AwaitEvent(z.literal(n));
  } else {
    yield* AwaitEvent(z.literal(n + 1));
  }
}

function* bla() {
  const n = yield* AwaitEvent(z.number().describe('number'));
  const b = yield* AwaitEvent(z.boolean().describe('bool'));
  if (b) {
    yield* AwaitEvent(z.literal(n).describe(`value ${n}`));
  } else {
    yield* AwaitEvent(z.literal(n + 1).describe(`value ${n + 1}`));
  }
}

function runGenerator(previousInput: any, gen: Generator<z.ZodTypeAny>): Proposition {
  const val = gen.next(previousInput);
  if (!val.done) {
    return Then(val.value, (input) => runGenerator(input, gen));
  } else {
    return Done;
  }
}

function testtest() {
  runGenerator(undefined, bla());
}

function* AwaitEvent<I extends z.ZodTypeAny>(schema: I): Generator<I, z.infer<I>, z.infer<I>> {
  const value = yield schema;
  return value;
}

function test(): Proposition {
  return {
    __kind: 'then',
    then(cont) {
      return cont(z.number(), (n) => {
        return {
          __kind: 'then',
          then(cont) {
            return cont(z.literal(n), (_) => Done);
          },
        };
      });
    },
  };
}

function consumeUntil<I extends z.ZodTypeAny>(
  schema: I,
  inputs: any[],
): [boolean, z.infer<I>, any[]] {
  console.log(`waiting for value of schema ${schema.description}`);
  let first;
  let result;
  do {
    first = inputs.shift();
    result = schema.safeParse(first);
    // console.log(`got result ${JSON.stringify(result)} when parsing ${JSON.stringify(first)}`)
  } while (!result.success && inputs.length > 0);

  return [result.success, result.data, inputs];
}

function test2(prop: Proposition, inputs: any[]) {
  if (prop.__kind === 'done') {
    console.log('match!');
    return;
  } else if (prop.__kind === 'then') {
    prop.then((schema, run) => {
      const [success, matched_value, tail] = consumeUntil(schema, inputs);

      if (success) {
        console.log(`matched ${matched_value}`);
        return test2(run(matched_value), tail);
      } else {
        console.log('no match, inputs exhausted!');
        return undefined;
      }
    });
    // const [matched_value, tail] = consumeUntil(schema, inputs);
    // if (matched_value) {
    //     prop.thenSchema(matched_value, (schema2, prop2) => test2(schema2, prop2, tail))
    // } else {
    //     console.log('no match, inputs exhausted!');
    // }
  }
}

function test3() {
  test2(ex(), [2, 3, 2]);
}

// test3()

test2(runGenerator(undefined, bla()), [2, 3, false, 4, 5, 3]);

// console.log(`${JSON.stringify(z.boolean().safeParse(false))}`)
