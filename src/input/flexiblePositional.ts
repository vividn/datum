import { DataArgs } from "./dataArgs";

export type PosArgType = "required" | "optional" | false;

// Allows positional keys to be treated like dataArgs, so that other keys can be specified before hand (like "proj=linux").
// This gives much more flexibility
// NOTE: Modifies the dataArgs in place!!
// Be sure to call in reverse order and also optional arguments first
export function flexiblePositional<T extends DataArgs, K extends keyof T>(
  args: T,
  positionalKey: K,
  argType: PosArgType,
  keyNameInData?: string
): void {
  keyNameInData ??= String(positionalKey);
  if (args[positionalKey] === undefined) {
    return;
  }
  if (argType) {
    args[argType] = [keyNameInData].concat(args[argType] ?? []);
  }
  args.data ??= [];
  args.data.unshift(args[positionalKey] as string | number);
  delete args[positionalKey];
}
