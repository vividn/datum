import { Argv } from "yargs";
import { baseArgs } from "./baseArgs";

type ParsedYargs = {
  [x: string]: unknown;
  _?: (string | number)[];
  $0?: string;
};
export async function parseArgs<A extends ParsedYargs>(
  args: A | string | string[],
  parser: (yargs: Argv) => Argv,
  command: string
): Promise<A> {
  if (!(typeof args === "string") && !Array.isArray(args)) {
    return args;
  }
  if (typeof args === "string") {
    args = 
  }
  if (Array.isArray(args) && args[0] !== command) {
    args = [command, ...args];
  }
  return (await parser(baseArgs).parseAsync(args)) as A;
}
