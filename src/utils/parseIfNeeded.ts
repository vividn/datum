import { ArgumentParser } from "argparse";
import { parse as shellParse } from "shell-quote";

export function parseIfNeeded<T>(
  parser: ArgumentParser,
  argsOrCli: T | string | string[],
  preparsed?: Partial<T>,
): T {
  if (typeof argsOrCli === "string" || Array.isArray(argsOrCli)) {
    const argArray =
      typeof argsOrCli === "string"
        ? (shellParse(argsOrCli) as string[])
        : argsOrCli;
    console.debug({argArray, preparsed})
    return parser.parse_intermixed_args(argArray, preparsed);
  }
  return preparsed ? ({ ...preparsed, ...argsOrCli } as T) : argsOrCli;
}
