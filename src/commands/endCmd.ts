import { EitherDocument } from "../documentControl/DatumDocument";
import { StartCmdArgs } from "./startCmd";
import { switchCmd } from "./switchCmd";
import set from "lodash.set";
import { ArgumentParser } from "argparse";
import { durationArgs } from "../input/durationArgs";
import { occurArgs } from "./occurCmd";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const endArgs = new ArgumentParser({
  parents: [occurArgs, durationArgs],
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
  preparsed?: Partial<EndCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(endCmdArgs, args, preparsed);
  args.cmdData ??= {};
  set(args.cmdData, "state.id", false);
  return await switchCmd({ ...args });
}
