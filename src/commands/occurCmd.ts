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
import { handleTimeArgs, TimeArgs } from "../input/timeArgs";
import { addArgs, AddCmdArgs } from "./addCmd";
import { parseBaseData } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = [
  "occur <field> [duration] [data..]",
  "occur --moment <field> [data..]",
];
export const desc = "add an occur document";

export function builder(yargs: Argv): Argv {
  return addArgs(yargs)
    .options({
      moment: {
        describe:
          "don't interpret the first argument after field as a duration",
        nargs: 0,
      },
    })
    .positional("field", {
      describe: "what is being tracked",
      type: "string",
      nargs: 1,
    })
    .positional("duration", {
      describe:
        "how long the event lasted, default units is minutes, but other forms can be used." +
        " 5 = 5 minutes, 5h = 5 hours, 5:35:35 = 5 hours 35 minutes and 35 seconds, etc." +
        'a single period "." can be used to indicate no duration (for example to allow' +
        " entering in of other data without specifying a duration)",
      type: "string",
      nargs: 1,
    });
}

export type OccurCmdArgs = AddCmdArgs &
  TimeArgs & {
    occurTime?: string;
    moment?: boolean;
  };

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  console.log({ args });
  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  const parsedData = parseBaseData(args.baseData);
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  return { _id: "asdf", _rev: "" };
  // return await addCmd({ ...args, baseData: parsedData });
}
