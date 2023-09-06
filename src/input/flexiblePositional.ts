import { DataArgs } from "./dataArgs";

export function flexiblePositional<T extends DataArgs, K extends keyof T>(
  args: T,
  positionalKey: K,
  argType: "optional" | "required",
  keyNameInData?: string
): void {
  // NOTE: Modifies the dataArgs in place!!
  keyNameInData ??= String(positionalKey);
  if (args[positionalKey] === undefined) {
    return;
  }
  args.data ??= [];
  args.data.unshift(args[positionalKey] as string | number);

  args[argType] = [keyNameInData].concat(args[argType] ?? []);
}
