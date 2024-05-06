import { splitFirst } from "../utils/splitFirst";

export function consolidateKeys(keys: string[]): string[] {
  return keys.reduce((acc, key) => {
    const [keyName] = splitFirst("=", key);
    const existingIndex = acc.findIndex((existingKeys) => {
      const [valueName] = splitFirst("=", existingKeys);
      return valueName === keyName;
    });
    if (existingIndex !== -1) {
      acc[existingIndex] = key;
    } else {
      acc.push(key);
    }
    return acc;
  }, [] as string[]);
}
