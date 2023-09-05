import { Argv } from "yargs";
import {
  DataOnlyPayload,
  DatumMetadata,
  DatumPayload,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { IdError, isCouchDbError } from "../errors";
import { defaults } from "../input/defaults";
import { newHumanId } from "../meta/newHumanId";
import chalk from "chalk";
import { addDoc, ConflictStrategyNames } from "../documentControl/addDoc";
import { buildIdStructure } from "../ids/buildIdStructure";
import { assembleId } from "../ids/assembleId";
import { defaultIdComponents } from "../ids/defaultIdComponents";
import { DataArgs, dataYargs, handleDataArgs } from "../input/dataArgs";
import { DateTime, Duration } from "luxon";
import { MainDatumArgs } from "../input/mainYargs";
import { addIdAndMetadata } from "../meta/addIdAndMetadata";

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
  return dataYargs(yargs).options({
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
  DataArgs & {
    noMetadata?: boolean;
    idPart?: string | string[];
    idDelimiter?: string;
    partition?: string;
    undo?: boolean;
    "force-undo"?: boolean;
    merge?: boolean;
    conflict?: ConflictStrategyNames;
  };

export async function addCmd(args: AddCmdArgs): Promise<EitherDocument> {
  const payloadData = handleDataArgs(args);
  const payload = addIdAndMetadata(payloadData, args);
  const _id = payload._id;

  const db = connectDb(args);

  const { undo, "force-undo": force } = args;
  if (undo || force) {
    let doc;
    try {
      doc = await db.get(_id);
    } catch (error) {
      // if the id involves a time, then there could be some slight difference in the id
      const idStructure = payload.meta?.idStructure ?? "";
      if (
        isCouchDbError(error) &&
        error.reason === "missing" &&
        idStructure.match(/%\??(create|modify|occur)Time%/)
      ) {
        // just get the next lowest id
        doc = (
          await db.allDocs({
            startkey: _id,
            descending: true,
            limit: 1,
            include_docs: true,
          })
        ).rows[0]?.doc;
        if (doc === undefined) {
          throw error;
        }
      } else {
        throw error;
      }
    }

    const fifteenMinutesAgo = DateTime.now().minus(
      Duration.fromObject({ minutes: 15 })
    );
    if (
      doc.meta?.createTime &&
      DateTime.fromISO(doc.meta.createTime) < fifteenMinutesAgo
    ) {
      if (!force) {
        // deletion prevention
        throw Error("Doc created more than fifteen minutes ago");
      }
      console.log("Doc created more than fifteen minutes ago");
    }
    await db.remove(doc._id, doc._rev);
    console.log(chalk.grey("DELETE: ") + chalk.red(doc._id));
    return doc;
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
