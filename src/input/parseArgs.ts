import { parse as shellParse } from "shell-quote";
import { mainYargs } from "./mainYargs";

type ParsedYargs = {
  [x: string]: unknown;
  _?: (string | number)[];
  $0?: string;
};
export async function parseArgs<A extends ParsedYargs>(
  args: A | string | string[],
  command?: string
): Promise<A> {
  console.log({type: typeof args, arr: Array.isArray(args), args});
  if (!(typeof args === "string") && !Array.isArray(args)) {
    return args;
  }
  if (typeof args === "string") {
    args = shellParse(args) as string[];
  }
  if (command && args[0] !== command) {
    args = [command, ...args];
  }
  return (await mainYargs().parse(args)) as A;
}
