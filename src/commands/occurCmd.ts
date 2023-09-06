import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { addArgs, addCmd, AddCmdArgs } from "./addCmd";
import { handleDataArgs, parseBaseData } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { startCmd } from "./startCmd";
import { endCmd } from "./endCmd";
import { isoDurationFromDurationStr } from "../time/parseDurationString";
import { flexiblePositional } from "../input/flexiblePositional";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { connectDb } from "../auth/connectDb";
import { primitiveUndo } from "../undo/primitiveUndo";

export const command = [
  "occur <field> [duration] [data..]",
  "occur <field> . [data..]",
  "occur --moment <field> [data..]",
  "occur --no-field [duration] [data..]",
  // "occur -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [duration] [optVal1] ... [optValN] [data..]",
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
  //TODO: Include more help here about placement of duration between required values and optional values
}
export function occurArgs(yargs: Argv): Argv {
  return durationArgs(baseOccurArgs(yargs));
}

export const builder: (yargs: Argv) => Argv = occurArgs;

export type OccurCmdArgs = BaseOccurArgs & DurationArgs;

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  flexiblePositional(args, "duration", "optional", "dur");
  const payloadData = handleDataArgs(args);

  if (payloadData.dur === "start") {
    return await startCmd(args);
  } else if (payloadData.dur === "end") {
    return await endCmd(args);
  }

  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    payloadData.occurTime = occurTime;
    payloadData.occurUtcOffset = utcOffset;
  }
  const payload = addIdAndMetadata(payloadData, args);

  const db = connectDb(args);

  const { undo, forceUndo } = args;
  if (undo || forceUndo) {
    return await primitiveUndo({
      db,
      payload,
      force: forceUndo,
      outputArgs: args,
    });
  }
  return await addCmd({ ...args, baseData: parsedData });
}
