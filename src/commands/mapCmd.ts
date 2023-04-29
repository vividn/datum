import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { inferType } from "../utils/inferType";
import { startsWith } from "../utils/startsWith";
import { MainDatumArgs } from "../input/mainYargs";
import { EitherPayload } from "../documentControl/DatumDocument";
import { renderView } from "../output/renderView";
import { Show } from "../input/outputArgs";

export const command = "map <mapName> [start] [end]";
export const desc = "display a map view or map reduce view";

export type MapCmdArgs = MainDatumArgs & {
  mapName: string;
  start?: string;
  end?: string;
  view?: string;
  reduce?: boolean;
  hid?: boolean;
  params?: PouchDB.Query.Options<any, any>;
};

export function mapCmdYargs(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the design document and the map function",
      type: "string",
    })
    .positional("start", {
      describe:
        "Limit results to keys that start with this value. If 'end' is also given, acts as the startkey parameter",
      type: "string",
    })
    .positional("end", {
      describe:
        "If given, then start acts as startkey and this acts as endkey parameter",
      type: "string",
    })
    .options({
      view: {
        describe:
          'use a different view than "default". TODO: Can also be speified in the mapName by using a slash i.e. map/view',
        type: "string",
        nargs: 1,
      },
      reduce: {
        describe:
          'whether to reduce, triggered directly by the "reduce" command',
        type: "boolean",
        hidden: true,
      },
      showId: {
        describe: "show the ids",
        type: "boolean"
      },
      params: {
        describe:
          "extra params to pass to the view function. See nano's DocumentViewParams type",
        alias: "p",
        coerce: (params): PouchDB.Query.Options<any, any> => {
          return inferType(params) as PouchDB.Query.Options<any, any>;
        },
      },
    });
}
export const builder = mapCmdYargs;

export async function mapCmd(
  args: MapCmdArgs
): Promise<PouchDB.Query.Response<EitherPayload>> {
  const db = connectDb(args);
  const startEndParams = args.end
    ? {
        startkey: inferType(args.start as string),
        endkey: inferType(args.end),
      }
    : args.start
    ? startsWith(inferType(args.start))
    : {};
  const viewParams: PouchDB.Query.Options<any, any> = {
    reduce: args.reduce ?? false,
    ...startEndParams,
    ...(args.params ?? {}),
  };
  // TODO: parse map name for /viewName
  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";
  const viewResult = useAllDocs
    ? await db.allDocs(viewParams)
    : await db.query(
        `${args.mapName}/${args.view ?? args.mapName}`,
        viewParams
      );
  if (args.show !== Show.None) {
    renderView(viewResult, args.);
  }
  return viewResult;
}
