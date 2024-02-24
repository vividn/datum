import { connectDb } from "../auth/connectDb";
import { inferType } from "../utils/inferType";
import { startsWith } from "../utils/startsWith";
import { MainDatumArgs } from "../input/mainArgs";
import { EitherPayload } from "../documentControl/DatumDocument";
import { renderView } from "../output/renderView";
import { outputArgs, Show } from "../input/outputArgs";
import { reverseViewParams } from "../utils/reverseViewParams";
import { ArgumentParser, SUPPRESS } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const mapArgs = new ArgumentParser({
  add_help: false,
});
mapArgs.add_argument("mapName", {
  help: "Name of the design document and the map function",
});
mapArgs.add_argument("start", {
  help: "Limit results to keys that start with this value. If 'end' is also given, acts as the startkey parameter",
  nargs: "?",
});
mapArgs.add_argument("end", {
  help: "If given, then start acts as startkey and this acts as endkey parameter",
  nargs: "?",
});
mapArgs.add_argument("--view", {
  help: 'use a different view than "default". TODO: Can also be speified in the mapName by using a slash i.e. map/view',
});
mapArgs.add_argument("--reduce", {
  help:
    'whether to reduce, triggered directly by the "reduce" command' || SUPPRESS,
  action: "store_true",
});
mapArgs.add_argument("--showId", {
  help: "show the ids",
  action: "store_true",
});
mapArgs.add_argument("--hid", {
  help: "show the humanIds",
  action: "store_true",
});
mapArgs.add_argument("--reverse", {
  help: "flips start and end and inverts descending param for easy reverse ordering of results",
  action: "store_true",
});
mapArgs.add_argument("--params", "-p", {
  help: "extra params to pass to the view function. See PouchDB.Query.Options type", // TODO: change to pouchDB
});

export const mapCmdArgs = new ArgumentParser({
  description: "display a map view or map reduce view",
  prog: "dtm map",
  usage: `%(prog)s <mapName> [start] [end]
  %(prog)s --view <view> <mapName> [start] [end]`,
  parents: [mapArgs, dbArgs, outputArgs],
});

export type MapCmdArgs = MainDatumArgs & {
  mapName: string;
  start?: string;
  end?: string;
  view?: string;
  reduce?: boolean;
  showId?: boolean;
  hid?: boolean;
  reverse?: boolean;
  params?: PouchDB.Query.Options<any, any>;
};

export async function mapCmd(
  args: MapCmdArgs | string | string[],
  preparsed?: Partial<MapCmdArgs>
): Promise<PouchDB.Query.Response<EitherPayload>> {
  args = parseIfNeeded(mapCmdArgs, args, preparsed);
  const db = connectDb(args);
  const startEndParams = args.end
    ? {
        startkey: inferType(args.start as string),
        endkey: inferType(args.end),
      }
    : args.start
    ? startsWith(inferType(args.start))
    : {};
  let viewParams: PouchDB.Query.Options<any, any> = {
    reduce: args.reduce ?? false,
    ...startEndParams,
    ...(args.params ?? {}),
  };
  if (args.hid && !viewParams.reduce) {
    viewParams.include_docs = true;
  }
  if (args.reverse) {
    viewParams = reverseViewParams(viewParams);
  }
  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";
  const viewResult = useAllDocs
    ? await db.allDocs(viewParams)
    : await db.query(
        `${args.mapName}/${args.view ?? args.mapName}`,
        viewParams
      );
  if (args.show !== Show.None) {
    renderView(viewResult, args.showId, args.hid);
  }
  return viewResult;
}
