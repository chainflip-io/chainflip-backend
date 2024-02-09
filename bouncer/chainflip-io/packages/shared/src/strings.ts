import type { Asset } from './enums';
import type { RpcAsset } from './node-apis/RpcClient';

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

export const transformAsset = (asset: Asset): RpcAsset =>
  (asset[0] + asset.slice(1).toLowerCase()) as Capitalize<Lowercase<Asset>>;
