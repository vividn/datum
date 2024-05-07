import { DataArgs } from "./dataArgs";

type KeyPosition = "prepend" | "append" | false;

// Allows positional keys to be treated like dataArgs, so that other keys can be specified before hand (like "proj=linux").
// This gives much more flexibility
// NOTE: Modifies the dataArgs in place!!
// Be sure to call in reverse order for prepend
export function flexiblePositional<T extends DataArgs, K extends keyof T>(
  args: T,
  keyName: K,
  keyPosition: KeyPosition,
  keyNameInData?: string,
): void {
  keyNameInData ??= String(keyName);
  if (args[keyName] === undefined) {
    return;
  }

  if (keyPosition === "prepend") {
    args.keys ??= [];
    args.keys.unshift(keyNameInData);
  } else if (keyPosition === "append") {
    args.keys ??= [];
    args.keys.push(keyNameInData);
  }

  args.data ??= [];
  args.data.unshift(args[keyName] as string | number);
  delete args[keyName];
}
