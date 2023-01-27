import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { renderView } from "../output/renderView";
import { MapCmdArgs } from "./mapCmd";
import { DocumentViewParams } from "nano";
import { inferType } from "../utils/inferType";
import { startsWith } from "../utils/startsWith";

export const command = "reduce <mapName> [groupLevel] [start] [end]";
export const desc = "display a reduction of a map";

export type ReduceCmdArgs = MapCmdArgs & {
  groupLevel?: number;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the mapName to use",
    })
    .positional("groupLevel", {
      describe: "how far to group the key arrays when reducing",
      type: "number",
    })
    .positional("start", {
      describe:
        "Limit results to keys that start with this value. If 'end' is also given, acts as the start_key parameter",
      type: "string",
    })
    .positional("end", {
      describe:
        "If given, then start acts as start_key and this acts as end_key parameter",
      type: "string",
    })
    .options({
      // TODO: DRY out with mapCmd
      view: {
        describe:
          'use a different view than "default". TODO: Can also be specified in the mapName by using a slash i.e. map/view',
        type: "string",
        nargs: 1,
      },
      reduce: {
        describe:
          'whether to reduce, triggered directly by the "reduce" command',
        type: "boolean",
      },
      params: {
        describe:
          "extra params to pass to the view function. See nano's DocumentViewParams type",
        type: "string",
        alias: "p",
      },
    });
}

export async function reduceCmd(args: ReduceCmdArgs): Promise<void> {
  const db = await connectDb(args);

  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";
  const startEndParams = args.end
    ? {
        start_key: inferType(args.start as string),
        end_key: inferType(args.end),
      }
    : args.start
    ? startsWith(inferType(args.start))
    : {};
  const viewParams: DocumentViewParams = {
    reduce: args.reduce ?? !useAllDocs,
    ...startEndParams,
    ...(args.params ? inferType(args.params) : {}),
  };

  // TODO: parse map name for /viewName
  let viewResult;
  if (useAllDocs) {
    const allDocs = await db.list({ ...viewParams });
    viewResult = {
      rows: [
        { key: null, value: (await db.list(viewParams)).rows.length, id: "" },
      ],
      total_rows: allDocs.total_rows,
      offset: allDocs.offset,
    };
  } else {
    viewResult = await db.view(
      args.mapName,
      args.view ?? "default",
      viewParams
    );
  }
  renderView(viewResult);
}
