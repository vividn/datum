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
import { inferType } from "../utils/inferType";
import { startCmd } from "./startCmd";
import { endCmd } from "./endCmd";
import { isoDurationFromDurationStr } from "../time/parseDurationString";

export const command = [
  "occur <field> [duration] [data..]",
  "occur --moment <field> [data..]",
];
export const desc = "add an occur document";

export type BaseOccurArgs = AddCmdArgs &
  TimeArgs & {
    field: string;
  };
export function baseOccurArgs(yargs: Argv): Argv {
  return timeYargs(addArgs(yargs)).positional("field", {
    describe: "what is being tracked",
    type: "string",
    nargs: 1,
  });
}

export type DurationArgs = {
  duration?: string | number;
  moment?: boolean;
};
export function durationArgs(yargs: Argv): Argv {
  return yargs
    .options({
      moment: {
        alias: "m",
        describe:
          "don't interpret the first argument after field as a duration",
        nargs: 0,
      },
    })
    .positional("duration", {
      describe:
        "how long the event lasted, default units is minutes, but other forms can be used." +
        " 5 = 5 minutes, 5h = 5 hours, 5:35:35 = 5 hours 35 minutes and 35 seconds, etc." +
        'a single period "." or empty string "" can be used to indicate no duration (for example to allow' +
        " entering in of other data without specifying a duration)",
      nargs: 1,
    });
}
export function occurArgs(yargs: Argv): Argv {
  return durationArgs(baseOccurArgs(yargs));
}

export const builder: (yargs: Argv) => Argv = occurArgs;

export type OccurCmdArgs = BaseOccurArgs & DurationArgs;

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  const parsedData = parseBaseData(args.baseData);
  if (args.duration !== undefined) {
    if (args.moment) {
      args.data ??= [];
      args.data.unshift(args.duration);
    } else {
      if (args.duration === "start") {
        delete args.duration;
        return await startCmd(args);
      }
      if (args.duration === "end") {
        delete args.duration;
        return await endCmd(args);
      }
      parsedData.dur = isoDurationFromDurationStr(String(args.duration));
    }
  }

  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  return await addCmd({ ...args, baseData: parsedData });
}
