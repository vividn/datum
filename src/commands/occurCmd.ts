import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { addArgs, AddCmdArgs } from "./addCmd";
import { handleDataArgs } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { startCmd } from "./startCmd";
import { endCmd } from "./endCmd";
import { flexiblePositional } from "../input/flexiblePositional";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { connectDb } from "../auth/connectDb";
import { primitiveUndo } from "../undo/primitiveUndo";
import { DurationArgs, durationArgs } from "../input/durationArgs";
import { FieldArgs } from "../input/fieldArgs";
import { addDoc } from "../documentControl/addDoc";

export const command = [
  "occur <field> [duration] [data..]",
  "occur <field> . [data..]",
  "occur --moment <field> [data..]",
  "occur --fieldless [duration] [data..]",
  // "occur -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [duration] [optVal1] ... [optValN] [data..]",
];
export const desc = "add an occur document";

export type BaseOccurArgs = AddCmdArgs & TimeArgs & FieldArgs;
export function baseOccurArgs(yargs: Argv): Argv {
  return timeYargs(addArgs(yargs)).positional("field", {
    describe: "what is being tracked",
    type: "string",
    nargs: 1,
  });
}

export function occurArgs(yargs: Argv): Argv {
  return durationArgs(baseOccurArgs(yargs));
}

export const builder: (yargs: Argv) => Argv = occurArgs;

export type OccurCmdArgs = BaseOccurArgs & DurationArgs;

export async function occurCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  flexiblePositional(
    args,
    "duration",
    !args.moment && !args.noTimestamp && "optional",
    "dur"
  );
  flexiblePositional(args, "field", !args.fieldless && "required");

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

  const { undo, forceUndo } = args;
  if (undo || forceUndo) {
    return await primitiveUndo({
      db,
      payload,
      force: forceUndo,
      outputArgs: args,
    });
  }
  const conflictStrategy = args.conflict ?? (args.merge ? "merge" : undefined);
  const doc = await addDoc({
    db,
    payload,
    conflictStrategy,
    outputArgs: args,
  });
  return doc;
}
