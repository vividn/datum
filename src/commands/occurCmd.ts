import { handleTimeArgs, timeArgs, TimeArgs } from "../input/timeArgs";
import { addCmd, AddCmdArgs, newDocArgs } from "./addCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { dataArgs } from "../input/dataArgs";
import { fieldArgs } from "../input/fieldArgs";

export const occurArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, newDocArgs, timeArgs, dataArgs],
});
export const occurCmdArgs = new ArgumentParser({
  description: "record the occurence of something at a single time point",
  prog: "datum occur",
  usage: `%(prog)s <field> [data..]
  %(prog)s --fieldless [data..]
  %(prog)s <field> -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [optVal1] ... [optValN] [data..]
`,
  parents: [occurArgs, dbArgs, outputArgs],
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
