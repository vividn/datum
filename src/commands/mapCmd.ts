import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { connectDb } from "../auth/connectDb";
import { renderView } from "../output/renderView";
import { DocumentViewParams } from "nano";
import { inferType } from "../utils/inferType";

export const command = "map <mapName>";
export const desc = "display a map view or map reduce view";

export type MapCmdArgs = BaseDatumArgs & {
  mapName: string;
  view?: string;
  reduce?: boolean;
  params?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the design document and the map function",
    })
    .options({
      view: {
        describe:
          'use a different view than "default". Can also be speified in the mapName by using a slash i.e. map/view',
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

export async function mapCmd(args: MapCmdArgs): Promise<void> {
  const db = await connectDb(args);
  const viewParams: DocumentViewParams = args.params
    ? inferType(args.params)
    : {};
  const viewResult = await db.view(args.mapName, args.view ?? "default", {
    reduce: args.reduce ?? false,
    ...viewParams,
  });
  renderView(viewResult);
}
