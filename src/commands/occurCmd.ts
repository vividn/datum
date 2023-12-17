import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs, timeYargs } from "../input/timeArgs";
import { addArgs, AddCmdArgs } from "./addCmd";
import { handleDataArgs } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { flexiblePositional } from "../input/flexiblePositional";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { connectDb } from "../auth/connectDb";
import { primitiveUndo } from "../undo/primitiveUndo";
import { addDoc } from "../documentControl/addDoc";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { compileState } from "../state/compileState";

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
  const db = connectDb(args);

  flexiblePositional(args, "field", !args.fieldless && "required");

  const { time: occurTime } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    args.occurTime = occurTime;
  }

  const payloadData = handleDataArgs(args);

  const payloadWithState = await compileState(db, payloadData);

  const payload = addIdAndMetadata(payloadWithState, args);

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
