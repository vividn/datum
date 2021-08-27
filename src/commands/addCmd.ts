import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import {
  DataOnlyPayload,
  DatumMetadata,
  DatumPayload,
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import connectDb from "../auth/connectDb";
import inferType from "../utils/inferType";
import { BaseDataError, isCouchDbError } from "../errors";
import { parseData } from "../parseData";
import { assembleId, buildIdStructure, defaultIdComponents } from "../ids";
import { defaults } from "../input/defaults";
import newHumanId from "../meta/newHumanId";
import { processTimeArgs, setTimezone } from "../timings";
import chalk from "chalk";
import addDoc from "../documentControl/addDoc";
import {
  updateStrategies,
  UpdateStrategyNames,
} from "../documentControl/combineData";
import { Show } from "../output";

export const command = "add [data..]";
export const desc = "add a document";

export type AddCmdArgs = BaseDatumArgs & {
  data?: (string | number)[];
  baseData?: string;
  date?: string;
  yesterday?: number;
  time?: string;
  quick?: number;
  timezone?: string;
  fullDay?: boolean;
  noTimestamp?: boolean;
  noMetadata?: boolean;
  field?: string;
  comment?: string | string[];
  idPart?: string | string[];
  idDelimiter?: string;
  partition?: string;
  undo?: boolean;
  merge?: boolean;
  update?: UpdateStrategyNames;
  required?: string | string[];
  optional?: string | string[];
  remainder?: string;
  stringRemainder?: boolean;
  lenient?: boolean;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("data", {
      describe:
        "The data to put in the document. " +
        "Data must include one argument for each key specified by --required. " +
        "Once required keys are filled, data will be assigned to keys specified by --optional. " +
        'Additional data can be specified in a "key=data" format. ' +
        "Any data that does not have a key will be put in the key specified with --remainder, unless strict mode is on",
    })
    .options({
      "base-data": {
        describe:
          "base object on which additional keys are added. Fed through relaxed-json, but should still parse to an object. Use with --no-metadata for raw json input into couchdb. Default: {}",
        nargs: 1,
        alias: "b",
        type: "string",
      },

      // timing
      date: {
        describe:
          "date of the timestamp, use `+n` or `-n` for a date relative to today. If no time is specified with -t, -T is assumed.",
        alias: "d",
        nargs: 1,
        type: "string",
      },
      yesterday: {
        describe:
          "use yesterday's date. Equivalent to `-d yesterday`. Use multiple times to go back more days",
        alias: "y",
        type: "count",
      },
      time: {
        describe:
          "specify time of the timestamp, use `+n` or `-n` for a timestamp n minutes relative to now",
        alias: "t",
        nargs: 1,
        type: "string",
      },
      quick: {
        describe:
          "quick options for time, use multiple times. -q = 5 min ago, -qq = 10 min ago, etc.",
        alias: "q",
        type: "count",
      },
      timezone: {
        describe:
          "Set the timezone to use instead of local time. Accepts both timezone names (America/Chicago) and utc offsets '-7'",
        alias: "z",
        type: "string",
      },
      "full-day": {
        describe:
          "make an entry for the full day, without a specific timestamp, occurs also when -d is used without -t",
        alias: "D",
        type: "boolean",
      },
      "no-timestamp": {
        describe: "omit the occurTime from the data",
        alias: "T",
        type: "boolean",
      },
      "no-metadata": {
        describe: "do not include meta data in document",
        alias: "M",
        type: "boolean",
      },
      // data
      field: {
        describe:
          "field specifying what is being tracked, used by default as partition for the data, but can be changed with --partition",
        alias: "f",
        nargs: 1,
        type: "string",
      },
      comment: {
        describe: "comment to include in the data",
        alias: "c",
        nargs: 1,
        type: "string",
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
          " Like --id-field, can be used  multiple times to assemble a partition separated by --id-delimiter",
        type: "string",
      },

      // Change behavior
      undo: {
        describe: "undoes the last datum entry, can be combined with -f",
        alias: "u",
        type: "boolean",
      },
      merge: {
        describe:
          "on conflict with an existing document update with the merge strategy. Equivalent to `--update merge`",
        alias: "x",
        type: "boolean",
        conflicts: "update",
      },
      update: {
        describe: `on conflict, update with given strategy.`,
        alias: "X",
        type: "string",
        choices: Object.keys(updateStrategies),
      },
      // "force-undo": {
      //   describe:
      //     "forces an undo, even if the datapoint was entered more than 15 minutes ago",
      //   alias: "U",
      //   type: "boolean",
      // },

      // Data
      required: {
        describe:
          "Add a required key to the data, will be filled with first keyless data. If not enough data is specified to fill all required keys, an error will be thrown",
        alias: ["K", "req"],
        type: "string",
        nargs: 1,
      },
      optional: {
        describe:
          "Add an optional key to the data, will be filled with first keyless data. A default value can be specified with an '=', e.g., -k key=value",
        alias: ["k", "opt"],
        type: "string",
        nargs: 1,
      },
      remainder: {
        describe:
          "Any extra data supplied will be put into this key as an array. When --lenient is specified, defaults to 'extraData'",
        alias: ["rem", "R"],
        type: "string",
        nargs: 1,
      },
      "string-remainder": {
        describe:
          "Remainder data will be a space-concatenated string rather than an array",
        alias: "S",
        type: "boolean",
      },
      lenient: {
        //TODO: Invert this to be strict
        describe: "Allow extra data without defined keys",
        type: "boolean",
        alias: "l",
      },
    });
}

export async function addCmd(args: AddCmdArgs): Promise<EitherDocument> {
  // Calculate timing data early to make occurTime more exact
  const { timeStr: occurTime, utcOffset } = !args.noTimestamp
    ? processTimeArgs(args)
    : {
        timeStr: undefined,
        utcOffset: setTimezone(args.timezone),
      };

  const {
    data: argData = [],
    field,
    comment,
    required,
    optional,
    remainder,
    stringRemainder,
    lenient,
  } = args;

  const baseData = args.baseData ? inferType(args.baseData) : {};
  if (typeof baseData !== "object" || baseData === null) {
    throw new BaseDataError("base data not a valid object");
  }

  const payloadData = parseData({
    argData,
    field,
    comment,
    required,
    optional,
    remainder,
    stringRemainder,
    lenient,
    baseData,
  });
  if (occurTime !== undefined) {
    payloadData.occurTime = occurTime;
  }

  const { defaultIdParts, defaultPartitionParts } = defaultIdComponents({
    data: payloadData,
  });

  const idStructure = buildIdStructure({
    idParts: args.idPart ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
    partition: args.partition ?? defaultPartitionParts,
  });

  const { noMetadata } = args;
  let meta: DatumMetadata | undefined = undefined;
  if (!noMetadata) {
    meta = {
      humanId: newHumanId(),
      random: Math.random(),
    };

    meta.utcOffset = utcOffset;

    // don't include idStructure if it is just a raw string (i.e. has no field references in it)
    // that would be a waste of bits since _id then is exactly the same
    if (idStructure.match(/(?<!\\)%/)) {
      meta.idStructure = idStructure;
    }
  }

  const payload: EitherPayload =
    meta !== undefined
      ? ({ data: payloadData, meta: meta } as DatumPayload)
      : ({ ...payloadData } as DataOnlyPayload);

  const _id = assembleId({
    payload,
    idStructure: idStructure,
  });
  payload._id = _id;

  const db = connectDb(args);

  const { undo } = args;
  if (undo) {
    let doc;
    try {
      doc = await db.get(_id);
    } catch (error) {
      // if the id involves a time, then there could be some slight difference in the id
      if (
        isCouchDbError(error) && error.reason === "missing" &&
        idStructure.match(/%\??(create|modify|occur)Time%/)
      ) {
        // just get the next lowest id
        doc = (
          await db.list({
            start_key: _id,
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
    await db.destroy(doc._id, doc._rev);
    console.log(chalk.grey("DELETE: ") + chalk.red(doc._id));
    return doc;
  }

  const conflictStrategy = args.update ?? (args.merge ? "merge" : undefined);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;
  const doc = await addDoc({
    db,
    payload,
    conflictStrategy,
    show,
  });
  return doc;
}

export default addCmd;
