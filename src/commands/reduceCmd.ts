import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { renderView } from "../output/renderView";
import { MapCmdArgs } from "./mapCmd";
import { DocumentViewParams } from "nano";
import { inferType } from "../utils/inferType";

export const command = "reduce <mapName> [groupLevel]";
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
    .options({
      // TODO: DRY out with mapCmd
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

export async function reduceCmd(args: ReduceCmdArgs): Promise<void> {
  const db = await connectDb(args);
  const viewParams: DocumentViewParams = args.params
    ? inferType(args.params)
    : {};
  const viewResult = await db.view(args.mapName, args.view ?? "default", {
    reduce: true,
    group_level: args.groupLevel,
    ...viewParams,
  });
  renderView(viewResult);
}
