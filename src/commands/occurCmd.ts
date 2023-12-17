import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { addArgs, addCmd, AddCmdArgs } from "./addCmd";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = [
  "occur <field> [data..]",
  "occur --fieldless [duration] [data..]",
  // "occur -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [duration] [optVal1] ... [optValN] [data..]",
];
export const desc = "add an occur document";

export type OccurCmdArgs = AddCmdArgs & TimeArgs;
export function occurArgs(yargs: Argv): Argv {
  return timeYargs(addArgs(yargs));
}

export const builder: (yargs: Argv) => Argv = occurArgs;

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  const { time: occurTime } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    args.occurTime = occurTime;
  }

  return addCmd(args);
}
