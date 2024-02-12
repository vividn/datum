import { EitherDocument } from "../documentControl/DatumDocument";
import { switchCmd } from "./switchCmd";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { DurationArgs, durationArgs } from "../input/durationArgs";
import set from "lodash.set";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const startArgs = new ArgumentParser({
  add_help: false,
  parents: [occurArgs, durationArgs],
});

export const startCmdArgs = new ArgumentParser({
  description: "record the start of something that occurs in blocks of time",
  prog: "dtm start",
  usage: `%(prog)s <field> [duration] [data..]
  %(prog)s <field> . [data..]
  %(prog)s <field> -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [optVal1] ... [optValN] [duration] [data..]
`,
  parents: [startArgs],
});

export type StartCmdArgs = OccurCmdArgs & DurationArgs;

export async function startCmd(
  args: StartCmdArgs | string | string[],
  preparsed?: Partial<StartCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(startCmdArgs, args, preparsed);
  args.cmdData ??= {};
  set(args.cmdData, "state.id", true);
  return await switchCmd({ ...args });
}
