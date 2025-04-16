import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import {
  addDoc,
  conflictChoices,
  ConflictStrategyNames,
} from "../documentControl/addDoc";
import { dataArgs, DataArgs, handleDataArgs } from "../input/dataArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { primitiveUndo } from "../undo/primitiveUndo";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { compileState } from "../state/compileState";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const newDocArgs = new ArgumentParser({
  add_help: false,
});
newDocArgs.add_argument("--no-metadata", "-M", {
  help: "do not include meta data in document",
  action: "store_true",
  dest: "noMetadata",
});
newDocArgs.add_argument("--id-part", "--id", {
  help:
    "Which field(s) to use for the _id field in the document." +
    " Can either be a single string with fields delimited by --id-delimiter" +
    " or can be used multiple times to progressively assemble an id delimited by --id-delimiter",
  action: "append",
  dest: "idParts",
});
newDocArgs.add_argument("--id-delimiter", {
  help: "spacer between fields in the id",
});
newDocArgs.add_argument("--undo", "-u", {
  help: "undoes the last datum entry",
  action: "store_true",
});
newDocArgs.add_argument("--force-undo", "-U", {
  help: "forces an undo, even if the datapoint was entered more than 15 minutes ago",
  action: "store_true",
  dest: "forceUndo",
});
newDocArgs.add_argument("--merge", "-x", {
  help: "on conflict with an existing document update with the merge strategy. Equivalent to `--update merge`",
  action: "store_const",
  const: "merge",
  dest: "conflict",
});
newDocArgs.add_argument("--conflict", "-X", {
  help: `on conflict, update with given strategy.`,
  choices: conflictChoices,
});

export const addArgs = new ArgumentParser({
  add_help: false,
  parents: [fieldArgs, newDocArgs, dataArgs],
});
export const addCmdArgs = new ArgumentParser({
  description: "add a document",
  prog: "datum add",
  usage: `%(prog)s <field> [data..]
  %(prog)s --fieldless [data..]
  %(prog)s <field> -k <reqKey> -k <optKey>=[defaultValue] ... <reqValue> [optValue] [data..]
`,
  parents: [addArgs, outputArgs, dbArgs],
});

export type AddCmdArgs = MainDatumArgs &
  FieldArgs &
  DataArgs & {
    noMetadata?: boolean;
    idParts?: string[];
    idDelimiter?: string;
    undo?: boolean;
    forceUndo?: boolean;
    merge?: boolean;
    conflict?: ConflictStrategyNames;
  };

export async function addCmd(
  args: AddCmdArgs | string | string[],
  preparsed?: Partial<AddCmdArgs>,
): Promise<EitherDocument> {
  args = parseIfNeeded(addCmdArgs, args, preparsed);
  const db = connectDb(args);

  flexiblePositional(args, "field", "field", args.fieldless);

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
