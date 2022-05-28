import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { datumV1View } from "../views/datumViews";
import { DocumentScope, DocumentViewResponse } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { flatten } from "table/dist/src/utils";

export const command = "v1 [field..]";
export const description =
  "export in the style of the old datum format. Outputs to stdout unless --output-file or --output-dir is given";

export type V1CmdArgs = BaseDatumArgs & {
  field: string[];
  outputFile?: string;
  outputDir?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("field", {
      describe:
        "field of the data. Corresponds the to the file in v1. Can list multiple. If none specified, will do all fields",
    })
    .options({
      "output-file": {
        description:
          "Path where the file should be written. Can be absolute or relative to to outputDir or cwd",
        type: "string",
        alias: ["o"],
      },
      "output-dir": {
        description:
          "Where to write the output files. if outputFile is not specified, data will be written to {{field}}.tsv",
        type: "string",
        alias: ["O"],
      },
    });
}

export async function v1Cmd(args: V1CmdArgs): Promise<void> {
  const db = await connectDb(args);

  if (args.field === undefined) {
    const { rows } = await db.view<string[]>(datumV1View.name, "default");
    if (!args.outputDir && !args.outputFile) {
      rows.reduce((currentField, row) => {
        const rowField = row.key[0];
        if (rowField !== currentField) {
          console.log("");
          console.log(`#### field: ${rowField}`);
          console.log(createHeader(rowField));
        }
        console.log(row.value.join("\t"));
        return rowField;
      }, "");
      return;
    }
    const skipFieldColumn = args.outputDir && !args.outputFile;
  }

  const viewResult = await db.view(datumV1View.name, "default", {
    // start_key: [args.field],
    // end_key: [args.field, "\uffff"],
    keys: [["apple"], ["coconut"]],
  });
  viewResult.rows
    .map((row) => [row.key[0], ...(row.value as string[])].join("\t"))
    .forEach((row) => console.log(row));
}

function createHeader(field: string): string {
  const standardSet = ["Date", "Time", "Offset", "Minutes"];
  switch (field) {
    case "activity":
      return standardSet.concat(["Activity", "Project"]).join("\t");

    case "environment":
      return standardSet.concat(["Category"]).join("\t");

    case "call":
      return standardSet.concat(["Format"]).join("\t");

    case "consume":
      return standardSet.concat(["Media"]).join("\t");

    case "hygiene":
      return standardSet.concat(["Activity"]).join("\t");

    default:
      return standardSet.join("\t");
  }
}

async function getRows(
  fields: string[],
  db: DocumentScope<EitherPayload>
): Promise<DocumentViewResponse<string[], EitherPayload>["rows"]> {
  if (fields.length === 0) {
    return (await db.view<string[]>(datumV1View.name, "default")).rows;
  }
  const groupedRows = await Promise.all(fields.map((field) => {
    return (await db.view<string[]>(datumV1View.name, "default", {
          start_key: [field],
          end_key: [field, "\uffff"],
        })
      ).rows;
  }));
  return flatten(groupedRows);
}
