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
import { updateLastDocsRef } from "../documentControl/lastDocs";

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
    delete payloadData.dur;
    args.baseData = payloadData; // should already be the case, but here for clarity and safety
    return await startCmd(args);
  } else if (payloadData.dur === "end") {
    delete payloadData.dur;
    args.baseData = payloadData;
    return await endCmd(args);
  }

  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    payloadData.occurTime = occurTime;
    payloadData.occurUtcOffset = utcOffset;
  }
  const payload = addIdAndMetadata(payloadData, args);

  // update now in case the addDoc fails due to conflict
  await updateLastDocsRef(db, payload._id);

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

  // if addDoc has changed the id (e.g. because it relies on the modifiyTime), update lastDocRef again
  // TODO: if changing lastDoc to history may need to change this to overwrite first update
  if (doc._id !== payload._id) {
    await updateLastDocsRef(db, doc._id);
  }

  return doc;
}
