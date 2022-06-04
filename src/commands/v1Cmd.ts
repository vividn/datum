import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { datumV1View } from "../views/datumViews";
import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { flatten } from "table/dist/src/utils";
import { ViewRow } from "../utils/utilityTypes";
import * as fs from "fs";
import path from "path";

export const command = "v1 [field..]";
export const description =
  "export in the style of the old datum format. Outputs to stdout unless --output-file or --output-dir is given";

export type V1CmdArgs = BaseDatumArgs & {
  field: string[];
  outputDir?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("field", {
      describe:
        "field of the data. Corresponds the to the file in v1. Can list multiple. If none specified, will do all fields",
    })
    .options({
      "output-dir": {
        description:
          "Where to write the output files. data will be written to {{field}}.tsv",
        type: "string",
        alias: ["O"],
      },
    });
}

export async function v1Cmd(args: V1CmdArgs): Promise<void> {
  const db = await connectDb(args);

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
  const { fd: finalFd } = rows.reduce(
    (state: { currentField?: string; fd: number }, row) => {
      let { currentField, fd } = state;
      const rowField = row.key[0];
      if (rowField !== currentField) {
        closeFd(fd);
        fd = openFd(rowField);
        currentField = rowField;
      }

      return { currentField, fd };
    },
    { currentField: undefined, fd: 0 }
  );

  closeFd(finalFd);
  return;
}

function createHeader(field: string): string[] {
  const standardSet = ["Date", "Time", "Offset", "Minutes"];
  switch (field) {
    case "activity":
      return standardSet.concat(["Activity", "Project"]);

    case "environment":
      return standardSet.concat(["Category"]);

    case "call":
      return standardSet.concat(["Format"]);

    case "consume":
      return standardSet.concat(["Media"]);

    case "hygiene":
      return standardSet.concat(["Activity"]);

    default:
      return standardSet;
  }
}

async function getRows(
  fields: string[],
  db: DocumentScope<EitherPayload>
): Promise<ViewRow<string[]>[]> {
  if (fields.length === 0) {
    return (await db.view<string[]>(datumV1View.name, "default")).rows;
  }
  const groupedRows = await Promise.all(
    fields.map(async (field) => {
      return (
        await db.view<string[]>(datumV1View.name, "default", {
          start_key: [field],
          end_key: [field, "\uffff"],
        })
      ).rows;
    })
  );
  return flatten(groupedRows);
}
