export const sorter =
  <T>(key: keyof T, dir: 'asc' | 'desc' = 'asc') =>
  (a: T, b: T) => {
    let result = 0;

    if (a[key] < b[key]) {
      result = -1;
    } else if (a[key] > b[key]) {
      result = 1;
    }

    return dir === 'asc' ? result : -result;
  };
