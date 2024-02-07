import { EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { addDoc, conflictChoices, ConflictStrategyNames } from "../documentControl/addDoc";
import { dataArgs, DataArgs, handleDataArgs } from "../input/dataArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";
import { primitiveUndo } from "../undo/primitiveUndo";
import { FieldArgs, fieldArgs } from "../input/fieldArgs";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { compileState } from "../state/compileState";
import { ArgumentParser, SubParser } from "argparse";

export function addArgs(parser: ArgumentParser): ArgumentParser {
  const argparser = dataArgs(fieldArgs(parser));
  argparser.add_argument("--no-metadata", "-M", {
    help: "do not include meta data in document",
    action: "store_true",
    dest: "noMetadata",
  });
  argparser.add_argument("--id-part", "--id", {
    help:
      "Which field(s) to use for the _id field in the document." +
      " Can either be a single string with fields delimited by --id-delimiter" +
      " or can be used multiple times to progressively assemble an id delimited by --id-delimiter",
    action: "append",
    dest: "idPart",
  });
  argparser.add_argument("--id-delimiter", {
    help: "spacer between fields in the id",
  });
  argparser.add_argument("--partition", "-P", {
    help:
      "field to use for the partition (default: field, specified with -f)." +
      " Can be fields of data or raw strings surrounded by single quotes." +
      " Like --id-part, can be used  multiple times to assemble a partition separated by --id-delimiter",
  });
  argparser.add_argument("--undo", "-u", {
    help: "undoes the last datum entry",
    action: "store_true",
  });
  argparser.add_argument("--force-undo", "-U", {
    help: "forces an undo, even if the datapoint was entered more than 15 minutes ago",
    action: "store_true",
  });
  argparser.add_argument("--merge", "-x", {
    help: "on conflict with an existing document update with the merge strategy. Equivalent to `--update merge`",
    action: "store_const",
    const: "merge",
    dest: "conflict",
  });
  argparser.add_argument("--conflict", "-X", {
    help: `on conflict, update with given strategy.`,
    choices: conflictChoices,
  });
  return argparser;
}

export function addCmdParser(subparsers: SubParser) {
  const addCmd = subparsers.add_parser("add", {
    description: "add a document",
  });
  return addArgs(addCmd);
}

export const command = [
  "add <field> [data..]",
  "add --fieldless [data..]",
  "add <field> -K <reqKey1> ... -K <reqKeyN> -k <optKey1>[=defaultVal1] ... -k <optKeyN> <reqVal1> ... <reqValN> [optVal1] ... [optValN] [data..]",
];
export const desc = "add a document";

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

  flexiblePositional(args, "field", args.fieldless ? false : "required");

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
