import { DataArgs } from "./dataArgs";

// Allows positional keys to be treated like dataArgs, so that other keys can be specified before hand (like "proj=linux").
// This gives much more flexibility
// NOTE: Modifies the dataArgs in place!!
// Be sure to call in reverse order
export function flexiblePositional<T extends DataArgs, K extends keyof T>(
  args: T,
  argName: K,
  keyName: string,
  skipKey?: boolean,
): void {
  keyName ??= String(argName);
  if (args[argName] === undefined) {
    return;
  }

  if (!skipKey) {
    args.keys ??= [];
    args.keys.unshift(keyName);
  }

  args.data ??= [];
  args.data.unshift(args[argName] as string | number);
  delete args[argName];
}
