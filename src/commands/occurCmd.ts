import { handleTimeArgs, timeArgs, TimeArgs } from "../input/timeArgs";
import { addArgs, addCmd, AddCmdArgs } from "./addCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const occurArgs = new ArgumentParser({
  parents: [addArgs, timeArgs],
});
export const occurCmdArgs = new ArgumentParser({
  description: "record the occurence of something at a single time point",
  prog: "dtm occur",
  usage: `%(prog)s <field> [data..]
  %(prog)s --fieldless [data..]
  %(prog)s <field> -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [optVal1] ... [optValN] [data..]
`,
  parents: [occurArgs],
});

export type OccurCmdArgs = AddCmdArgs & TimeArgs;

export async function occurCmd(
  args: OccurCmdArgs | string | string[],
  preparsed?: Partial<OccurCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(occurCmdArgs, args, preparsed);
  const { time: occurTime } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    args.cmdData ??= {};
    args.cmdData.occurTime = occurTime;
  }

  return await addCmd(args);
}
