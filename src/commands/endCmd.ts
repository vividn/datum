import { EitherDocument } from "../documentControl/DatumDocument";
import { startArgs, StartCmdArgs } from "./startCmd";
import { switchCmd } from "./switchCmd";
import set from "lodash.set";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const endArgs = new ArgumentParser({
  add_help: false,
  parents: [startArgs],
});
export const endCmdArgs = new ArgumentParser({
  description: "record the end of something that occurs in blocks of time",
  prog: "dtm end",
  usage: `%(prog)s <field> [duration] [data..]
`,
  parents: [endArgs],
});

export type EndCmdArgs = StartCmdArgs;

export async function endCmd(
  args: StartCmdArgs | string | string[],
  preparsed?: Partial<EndCmdArgs>
): Promise<EitherDocument> {
  args = parseIfNeeded(endCmdArgs, args, preparsed);
  args.cmdData ??= {};
  set(args.cmdData, "state.id", false);
  return await switchCmd({ ...args });
}
