#!/usr/bin/env -S pnpm tsx

import { z } from 'zod';

type Sth<A> = <I extends z.ZodTypeAny>(schema: I, i: Proposition<I>) => A;

type Proposition<Input extends z.ZodTypeAny> =
  | {
      __kind: 'then';
      thenSchema: <A>(input: z.output<Input>, cont: Sth<A>) => A;
      // thenProp: (input: z.output<Input>) => any,
    }
  | {
      __kind: 'done';
      // done: <A>(cont: (input: z.infer<Input>) => A) => A
    };

// first integer, then boolean
function test(): Proposition<z.ZodNumber> {
  return {
    __kind: 'then',
    thenSchema(input, cont) {
      const schema = z.literal(input);
      type II = z.infer<typeof schema>;
      return cont(schema, {
        __kind: 'done',
      });
    },
  };
}

function consumeUntil<I extends z.ZodTypeAny>(schema: I, inputs: any[]): [z.infer<I>, any[]] {
  let first;
  let result;
  do {
    first = inputs.shift();
    result = schema.safeParse(first);
  } while (!result.data && inputs.length > 0);

  return [result.data, inputs];
}

function test2<I0 extends z.ZodTypeAny>(schema: I0, prop: Proposition<I0>, inputs: any[]) {
  console.log(`got inputs ${inputs}`);
  if (prop.__kind === 'done') {
    console.log('match!');
    return;
  } else if (prop.__kind === 'then') {
    const [matched_value, tail] = consumeUntil(schema, inputs);
    if (matched_value) {
      prop.thenSchema(matched_value, (schema2, prop2) => test2(schema2, prop2, tail));
    } else {
      console.log('no match, inputs exhausted!');
    }
  }
}

function test3() {
  test2(z.number(), test(), [2]);
}

const schema = z.number();
const sc1 = z.literal(2);

test3();
