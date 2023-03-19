import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { DocumentViewParams, DocumentViewResponse } from "nano";
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
  params?: DocumentViewParams;
};

export function mapCmdYargs(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the design document and the map function",
      type: "string",
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
      params: {
        describe:
          "extra params to pass to the view function. See nano's DocumentViewParams type",
        alias: "p",
        coerce: (params): DocumentViewParams => {
          return inferType(params) as DocumentViewParams;
        },
      },
    });
}
export const builder = mapCmdYargs;

export async function mapCmd(
  args: MapCmdArgs
): Promise<DocumentViewResponse<unknown, EitherPayload<unknown>>> {
  const db = await connectDb(args);
  const startEndParams = args.end
    ? {
        start_key: inferType(args.start as string),
        end_key: inferType(args.end),
      }
    : args.start
    ? startsWith(inferType(args.start))
    : {};
  const viewParams: DocumentViewParams = {
    reduce: args.reduce ?? false,
    ...startEndParams,
    ...(args.params ?? {}),
  };
  // TODO: parse map name for /viewName
  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";
  const viewResult = useAllDocs
    ? await db.allDocs(viewParams)
    : await db.view(args.mapName, args.view ?? "default", viewParams);
  if (args.show !== Show.None) {
    renderView(viewResult);
  }
  return viewResult;
}
