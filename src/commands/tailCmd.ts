import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";
import {
  DatumData,
  DatumMetadata,
  EitherDocument,
  isDatumDocument,
  isDatumPayload,
} from "../documentControl/DatumDocument";
import { viewMap } from "../views/viewMap";
import { connectDb } from "../auth/connectDb";
import { occurTimeView } from "../views/datumViews";
import { getBorderCharacters, table } from "table";
import { DateTime, FixedOffsetZone } from "luxon";
import { humanTime } from "../time/humanTime";
import { interpolateFields } from "../ids/interpolateFields";

export const command = ["tail [field]"];
export const desc =
  "show the most recently occured/modified/created entries in the db";

export type TailCmdArgs = BaseDatumArgs & {
  num?: number;
  field?: string;
  metric?: "occur" | "create" | "modify";
  format?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.options({
    num: {
      alias: ["n", "number"],
      describe: "number of entries to show, defaults to 10",
      type: "number",
    },
    // TODO
    // field: {
    //   describe: "limit entries to a particular field of data",
    //   alias: "f",
    //   nargs: 1,
    //   type: "string",
    // },
    // date: {
    //   describe:
    //     "Show all that happened on a date instead of the most recent. Unless -n is specified, will return all",
    //   nargs: 1,
    //   type: "string",
    // },
    // metric: {
    //   describe: "which time to use for the sorting",
    //   choices: ["occur", "create", "modify"],
    //   alias: "m",
    //   type: "string",
    // },
    format: {
      describe:
        "custom format for outputting the data. To use fields in the data use %fieldName%, for fields in the metadata use %?fieldName%",
      type: "string",
    },
    // head: {
    //   describe: "show first rows instead of last rows",
    //   type: "boolean",
    // },
    // view: {
    //   describe: "specify a specific view to use instead of the built in time views",
    //   nargs: 1,
    //   type: "string"
    // }
  });
}

export async function tailCmd(args: TailCmdArgs): Promise<EitherDocument[]> {
  const db = connectDb(args);

  const limit = args.num ?? 10;
  const viewResults = await viewMap({
    db,
    datumView: occurTimeView,
    params: {
      descending: true,
      start_key: "\uffff\uffff",
      limit,
      include_docs: true,
    },
  });
  const rawRows = viewResults.rows.reverse();
  const docs: EitherDocument[] = rawRows.map((row) => row.doc!);
  const format = args.format;
  if (format) {
    //TODO: factor this out better, automatically extract all docs into a similar forat to simplify code base
    let data: DatumData;
    let meta: DatumMetadata | undefined;
    docs.forEach((doc) => {
      if (isDatumPayload(doc)) {
        data = doc.data as DatumData;
        meta = doc.meta;
      } else {
        data = doc as DatumData;
      }
      console.log(
        interpolateFields({ data, meta, format, useHumanTimes: true })
      );
    });
    return docs;
  }

  const headerRow = ["occurTime", "hid", "id"];
  const tableRows = [headerRow].concat(
    docs.map((doc) => {
      let occurTime, hid, id;
      if (isDatumDocument(doc)) {
        const data = doc.data;
        const meta = doc.meta;
        occurTime = data.occurTime
          ? humanTime(
              DateTime.fromISO(data.occurTime, {
                zone: meta.utcOffset
                  ? FixedOffsetZone.instance(60 * meta.utcOffset)
                  : undefined,
              })
            )
          : "";
        id = doc._id;
        hid = meta.humanId?.slice(0, 5) ?? "";
      } else {
        occurTime = doc.occurTime
          ? humanTime(DateTime.fromISO(doc.occurTime).toLocal())
          : "";
        hid = "";
        id = doc._id;
      }
      return [occurTime, hid, id];
    })
  );

  const output = table(tableRows, {
    border: getBorderCharacters("void"),
    columnDefault: {
      paddingLeft: 0,
      paddingRight: 1,
    },
    drawHorizontalLine: () => false,
  });

  console.log(output);

  return docs;
}
