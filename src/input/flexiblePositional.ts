import { DataArgs } from "./dataArgs";

type KeyPosition = "prepend" | "append" | false;

// Allows positional keys to be treated like dataArgs, so that other keys can be specified before hand (like "proj=linux").
// This gives much more flexibility
// NOTE: Modifies the dataArgs in place!!
// Be sure to call in reverse order for prepend
export function flexiblePositional<T extends DataArgs, K extends keyof T>(
  args: T,
  argName: K,
  keyPosition: KeyPosition,
  keyName?: string,
): void {
  keyName ??= String(argName);
  if (args[argName] === undefined) {
    return;
  }

  if (keyPosition === "prepend") {
    args.keys ??= [];
    args.keys.unshift(keyName);
  } else if (keyPosition === "append") {
    args.keys ??= [];
    args.keys.push(keyName);
  }

  args.data ??= [];
  args.data.unshift(args[argName] as string | number);
  delete args[argName];
}
