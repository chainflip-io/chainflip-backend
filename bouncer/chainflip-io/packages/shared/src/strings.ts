export type CamelCaseToSnakeCase<S extends string> =
  S extends `${infer T}${infer U}`
    ? `${T extends Capitalize<T>
        ? '_'
        : ''}${Lowercase<T>}${CamelCaseToSnakeCase<U>}`
    : S;

export const camelToSnakeCase = <const T extends string>(
  str: T,
): CamelCaseToSnakeCase<T> =>
  str.replace(
    /[A-Z]/g,
    (letter) => `_${letter.toLowerCase()}`,
  ) as CamelCaseToSnakeCase<T>;

export const toUpperCase = <const T extends string>(value: T) =>
  value.toUpperCase() as Uppercase<T>;

type ScreamingSnakeCaseToPascalCase<S extends string> =
  S extends `${infer T}_${infer U}`
    ? `${Capitalize<Lowercase<T>>}${ScreamingSnakeCaseToPascalCase<U>}`
    : Capitalize<Lowercase<S>>;

export const screamingSnakeToPascalCase = <const T extends string>(value: T) =>
  value
    .split('_')
    .map((word) => `${word[0].toUpperCase()}${word.slice(1).toLowerCase()}`)
    .join('') as ScreamingSnakeCaseToPascalCase<T>;
