/*
Let's see here.
I want a start command and then a corresponding stop command
Also an occur command
hmmmmm

for block data:
start <field>
end <field>
occur <field> [duration]
/// pause <field> <duration> -- like doing `occur <field> -duration`
if [duration] is "start" or "end", then automatically transform into the respective start or end type data

for block data in the end it should be relatively equivalent to using state data with states of true and false

for point data:
occur <field>

for state data:
switch <field> <state>
Should include information about what the last state was for mapreduce totalling later, must have some error checking function


 */

import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { addArgs, addCmd, AddCmdArgs } from "./addCmd";
import { parseBaseData } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = "occur <field> [..data]";
export const desc = "add an occur document";

export function builder(yargs: Argv): Argv {
  return timeYargs(addArgs(yargs)).options({});
}

export type OccurCmdArgs = AddCmdArgs & TimeArgs;

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  const parsedData = parseBaseData(args.baseData);
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  return await addCmd({ ...args, baseData: parsedData });
}
