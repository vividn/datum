import { connectDb } from "../auth/connectDb";
import { datumV1View } from "../views/datumViews";
import { EitherPayload } from "../documentControl/DatumDocument";
import * as fs from "fs";
import path from "path";
import { V1MapRow, V1ReduceRowGroup1 } from "../views/datumViews/datumV1";
import flatten from "lodash.flatten";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";

export const v1Args = new ArgumentParser({
  add_help: false,
});
v1Args.add_argument("field", {
  help: "field of the data. Corresponds the to the file in v1. Can list multiple. If none specified, will do all fields",
  nargs: "*",
});
v1Args.add_argument("--output-dir", "-O", {
  help: "Where to write the output files. data will be written to {{field}}.tsv",
  type: "str",
  dest: "outputDir",
});

export const v1CmdArgs = new ArgumentParser({
  description:
    "export in the style of the old datum format. Outputs to stdout unless --output-file or --output-dir is given",
  prog: "datum v1",
  usage: "%(prog)s [field..]",
  parents: [v1Args, dbArgs],
});

export type V1CmdArgs = MainDatumArgs & {
  field: string[];
  outputDir?: string;
};

export async function v1Cmd(
  argsOrCli: V1CmdArgs | string | string[],
  preparsed?: Partial<V1CmdArgs>,
): Promise<void> {
  const args = parseIfNeeded(v1CmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);

  function openFd(field: string): number {
    if (!args.outputDir) {
      // use stdOut if no output file specified
      console.log("");
      console.log(`#### field: ${field}`);
      return 1;
    }
    return fs.openSync(path.join(args.outputDir, field + ".tsv"), "w");
  }
  function closeFd(fd: number): void {
    // don't close stdOut or stdErr
    if (fd < 3) {
      return;
    }
    fs.close(fd, (err) => {
      if (err) {
        throw err;
      }
    });
  }
  const rows = await getRows(args.field, db);
  const columnCountRows = await getColumnCounts(args.field, db);
  const columnCounts = columnCountRows.reduce(
    (acc, row) => {
      acc[row.key[0]] = row.value;
      return acc;
    },
    {} as Record<string, number>,
  );
  const { fd: finalFd } = rows.reduce(
    (current: { currentField?: string; fd: number }, row) => {
      let { currentField, fd } = current;
      const rowField = row.key[0];
      if (rowField !== currentField) {
        closeFd(fd);
        fd = openFd(rowField);
        currentField = rowField;

        fs.writeSync(
          fd,
          createHeader(currentField, columnCounts[currentField]).join("\t") +
            "\n",
        );
      }
      fs.writeSync(fd, row.value.join("\t") + "\n");

      return { currentField, fd };
    },
    { currentField: undefined, fd: 0 },
  );

  closeFd(finalFd);
  return;
}

function createHeader(field: string, columnCount: number): string[] {
  const defaultHeader = ["Date", "Time", "Offset", "Minutes"];
  const extraFields = [];
  switch (field) {
    case "environment":
      extraFields.push("Category");
      break;

    default:
      extraFields.push("State");
      break;
  }
  return defaultHeader.concat(extraFields).slice(0, columnCount);
}

async function getRows(
  fields: string[],
  db: PouchDB.Database<EitherPayload>,
): Promise<V1MapRow[]> {
  if (fields.length === 0) {
    return (await db.query<string[]>(datumV1View.name, { reduce: false }))
      .rows as V1MapRow[];
  }
  const groupedRows = await Promise.all(
    fields.map(async (field) => {
      return (
        await db.query<any>(datumV1View.name, {
          startkey: [field],
          endkey: [field, "\uffff"],
          reduce: false,
        })
      ).rows;
    }),
  );
  return flatten(groupedRows);
}

async function getColumnCounts(
  fields: string[],
  db: PouchDB.Database<EitherPayload>,
): Promise<V1ReduceRowGroup1[]> {
  if (fields.length === 0) {
    return (
      await db.query<string[]>(datumV1View.name, {
        reduce: true,
        group_level: 1,
        group: true,
      })
    ).rows as V1ReduceRowGroup1[];
  }
  const groupedRows = await Promise.all(
    fields.map(async (field) => {
      return (
        await db.query<any>(datumV1View.name, {
          startkey: [field],
          endkey: [field, "\uffff"],
          reduce: true,
          group_level: 1,
          group: true,
        })
      ).rows;
    }),
  );
  return flatten(groupedRows);
}
