import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { subHumanIdView } from "../views/datumViews/humanId";
import mapOut from "../output/mapOut";
import connectDb from "../auth/connectDb";
import { Show } from "../output/output";

export const command = "map <mapName> [viewName]";
export const desc = "display a map view or map reduce view";

export type MapCmdArgs = BaseDatumArgs & {
  mapName: string;
  viewName: string;
};


export function builder(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the design document and the map function",
    })
    .positional("viewName", {
      default: "default"
    })
    .options({
      hello: {
        describe: "say hello",
        type: "string",
        nargs: 1,
      },
    });
}

export async function mapCmd(args: MapCmdArgs): Promise<void> {
  const db = connectDb(args);
  const view = await db.view(args.mapName, args.viewName, {reduce: false});
  const show: Show = args.showAll ? Show.All : args.show ?? Show.Standard;
  mapOut({view, show});
}

export default mapCmd;
