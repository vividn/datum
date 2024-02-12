import { ArgumentParser } from "argparse";
import { parse as shellParse } from "shell-quote";

export function parseIfNeeded<T>(
  parser: ArgumentParser,
  args: T | string | string[],
  preparsed?: Partial<T>,
): T {
  if (typeof args === "string" || Array.isArray(args)) {
    const argArray =
      typeof args === "string" ? (shellParse(args) as string[]) : args;
    return parser.parse_args(argArray, preparsed);
  }
  return preparsed ? ({ ...preparsed, ...args } as T) : args;
}
