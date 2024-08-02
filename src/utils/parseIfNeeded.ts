import { ArgumentParser } from "argparse";
import { parse as shellParse } from "shell-quote";

export function parseIfNeeded<T>(
  parser: ArgumentParser,
  argsOrCli: T | string | string[],
  preparsed?: Partial<T>,
  withSubparsers = false
): T {
  if (typeof argsOrCli === "string" || Array.isArray(argsOrCli)) {
    const argArray =
      typeof argsOrCli === "string"
        ? (shellParse(argsOrCli) as string[])
        : argsOrCli;
    if (withSubparsers) {
      return parser.parse_args(argArray, preparsed) as T;
    }
    return parser.parse_intermixed_args(argArray, preparsed);
  }
  return preparsed ? ({ ...preparsed, ...argsOrCli } as T) : argsOrCli;
}
