import { Argv } from "yargs";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { handleDataArgs } from "../input/dataArgs";
import { handleTimeArgs } from "../input/timeArgs";
import { connectDb } from "../auth/connectDb";
import { flexiblePositional } from "../input/flexiblePositional";
import { getLastState } from "../state/findLastState";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { primitiveUndo } from "../undo/primitiveUndo";
import { addDoc } from "../documentControl/addDoc";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { durationArgs, DurationArgs } from "../input/durationArgs";
import { DatumState } from "../views/datumViews/activeStateView";

export const command = [
  "switch <field> [state] [duration] [data..]",
  "switch --moment <field> [state] [data..]",
];
export const desc = "switch states of a given field";

export function builder(yargs: Argv): Argv {
  return durationArgs(occurArgs(yargs))
    .positional("state", {
      describe:
        "the state to switch to, it defaults to true--equivalent to start",
      type: "string",
      nargs: 1,
      default: "true",
    })
    .options({
      "last-state": {
        describe: "manually specify the last state being transitioned out of",
        nargs: 1,
        // TODO: add alias l here after switching lenient to strict
      },
    });
}

export type StateArgs = {
  state: DatumState;
  lastState?: DatumState;
};
export type SwitchCmdArgs = OccurCmdArgs & DurationArgs & StateArgs;

export async function switchCmd(args: SwitchCmdArgs): Promise<EitherDocument> {
  const db = await connectDb(args);
  flexiblePositional(
    args,
    "duration",
    !args.moment && !args.noTimestamp && "optional",
    "dur",
  );
  flexiblePositional(args, "state", "optional");
  flexiblePositional(args, "field", !args.fieldless && "required");
  const payloadData = handleDataArgs(args);

  const { time: occurTime } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    payloadData.occurTime = occurTime;
  }
  payloadData.lastState = await getLastState({
    db,
    field: payloadData.field,
    lastState: args.lastState,
    time: occurTime,
  });

  const payload = addIdAndMetadata(payloadData, args);
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
