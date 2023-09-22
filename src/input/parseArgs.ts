import { Argv } from "yargs";
import { baseArgs } from "./baseArgs";

type ParsedYargs = {
  [x: string]: unknown;
  _?: (string | number)[];
  $0?: string;
};
export async function parseArgs<A extends ParsedYargs>(
  args: A | string | string[],
  parser: (yargs: Argv) => Argv
): Promise<A> {
  if (!(typeof args === "string") && !Array.isArray(args)) {
    return args;
  }
  return (await parser(baseArgs).parseAsync(args)) as A;
}
