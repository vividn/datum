import { ArgumentParser } from "argparse";

export function parseIfNeeded<T>(
  parser: ArgumentParser,
  args: T | string | string[],
  preparsed?: Partial<T>,
): T {
  if (typeof args === "string" || Array.isArray(args)) {
    const argArray = typeof args === "string" ? args.split(" ") : args;
    return parser.parse_args(argArray, preparsed);
  }
  return args;
}
