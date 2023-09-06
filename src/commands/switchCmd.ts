import { Argv } from "yargs";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { handleDataArgs, parseBaseData } from "../input/dataArgs";
import { handleTimeArgs } from "../input/timeArgs";
import { getActiveState } from "../state/getActiveState";
import { connectDb } from "../auth/connectDb";
import { DateTime } from "luxon";
import { inferType } from "../utils/inferType";
import { addCmd } from "./addCmd";
import { flexiblePositional } from "../input/flexiblePositional";
import { getLastState } from "../state/findLastState";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { primitiveUndo } from "../undo/primitiveUndo";
import { addDoc } from "../documentControl/addDoc";

export const command = [
  "switch <field> <state> [duration] [data..]",
  "switch --moment <field> <state> [data..]",
];
export const desc = "switch states of a given field";

export function builder(yargs: Argv): Argv {
  return occurArgs(yargs)
    .positional("state", {
      describe: "the state to switch to",
      type: "string",
      nargs: 1,
    })
    .options({
      "last-state": {
        describe: "manually specify the last state being transitioned out of",
        nargs: 1,
        // TODO: add alias l here after switching lenient to strict
      },
    });
}

export type SwitchCmdArgs = OccurCmdArgs & {
  state: string | boolean | null;
  lastState?: string | boolean | null;
};

export async function switchCmd(args: SwitchCmdArgs): Promise<EitherDocument> {
  const db = await connectDb(args);
  flexiblePositional(args, "duration", "optional", "dur");
  flexiblePositional(args, "state", "required");
  flexiblePositional(args, "field", "required");
  const payloadData = handleDataArgs(args);

  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    payloadData.occurTime = occurTime;
    payloadData.occurUtcOffset = utcOffset;
  }

  payloadData.lastState = getLastState({db, field: payloadData.field, lastState: args.lastState, time: occurTime});

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
