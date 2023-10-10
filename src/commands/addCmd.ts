import { Argv } from "yargs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { addDoc, ConflictStrategyNames } from "../documentControl/addDoc";
import { DataArgs, dataYargs, handleDataArgs } from "../input/dataArgs";
import { MainDatumArgs } from "../input/mainYargs";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { primitiveUndo } from "../undo/primitiveUndo";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";

export const command = [
  "add <field> [data..]",
  "add --fieldless [data..]",
  "add <field> -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [optVal1] ... [optValN] [data..]",
];
export const desc = "add a document";

const conflictRecord: Record<ConflictStrategyNames, any> = {
  merge: "",
  useOld: "",
  preferOld: "",
  preferNew: "",
  useNew: "",
  removeConflicting: "",
  xor: "",
  intersection: "",
  append: "",
  prepend: "",
  appendSort: "",
  mergeSort: "",
  overwrite: "",
  delete: "",
};
const conflictChoices = Object.keys(conflictRecord);

export function addArgs(yargs: Argv): Argv {
  return dataYargs(fieldArgs(yargs)).options({
    "no-metadata": {
      describe: "do not include meta data in document",
      alias: "M",
      type: "boolean",
    },
    // id
    "id-part": {
      describe:
        "Which field(s) to use for the _id field in the document." +
        " Can either be a single string with fields delimited by --id-delimiter" +
        " or can be used multiple times to progressively assemble an id delimited by --id-delimiter",
      alias: ["id", "pk", "_id"],
      type: "string",
    },
    "id-delimiter": {
      describe: "spacer between fields in the id",
      type: "string",
    },
    partition: {
      describe:
        "field to use for the partition (default: field, specified with -f)." +
        " Can be fields of data or raw strings surrounded by single quotes." +
        " Like --id-part, can be used  multiple times to assemble a partition separated by --id-delimiter",
      alias: ["P"],
      type: "string",
    },

    // Undo
    undo: {
      describe: "undoes the last datum entry, can be combined with -f",
      alias: "u",
      type: "boolean",
    },
    "force-undo": {
      describe:
        "forces an undo, even if the datapoint was entered more than 15 minutes ago",
      alias: "U",
      type: "boolean",
    },

    merge: {
      describe:
        "on conflict with an existing document update with the merge strategy. Equivalent to `--update merge`",
      alias: "x",
      type: "boolean",
      conflicts: "conflict",
    },
    conflict: {
      describe: `on conflict, update with given strategy.`,
      alias: "X",
      type: "string",
      choices: conflictChoices,
    },
  });
}

export const builder: (yargs: Argv) => Argv = addArgs;

export type AddCmdArgs = MainDatumArgs &
  FieldArgs &
  DataArgs & {
    noMetadata?: boolean;
    idPart?: string | string[];
    idDelimiter?: string;
    partition?: string;
    undo?: boolean;
    forceUndo?: boolean;
    merge?: boolean;
    conflict?: ConflictStrategyNames;
  };

export async function addCmd(args: AddCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const fieldArgType = args.fieldless ? false : "required";
  flexiblePositional(args, "field", fieldArgType);
  const payloadData = handleDataArgs(args);

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
