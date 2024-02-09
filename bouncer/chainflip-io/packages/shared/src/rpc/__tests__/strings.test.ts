import { screamingSnakeToPascalCase } from '../../strings';

describe(screamingSnakeToPascalCase, () => {
  it.each([
    ['SOME_STRING', 'SomeString'],
    ['SOME_STRING_WITH_MORE_WORDS', 'SomeStringWithMoreWords'],
    ['FOO', 'Foo'],
  ])('properly formats %s', (input, expected) => {
    expect(screamingSnakeToPascalCase(input)).toBe(expected);
  });
});
