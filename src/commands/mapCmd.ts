import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { connectDb } from "../auth/connectDb";
import { renderView } from "../output/renderView";

export const command = "map <mapName>";
export const desc = "display a map view or map reduce view";

export type MapCmdArgs = BaseDatumArgs & {
  mapName: string;
};

export function builder(yargs: Argv): Argv {
  return yargs
    .positional("mapName", {
      describe: "Name of the design document and the map function",
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
  const db = await connectDb(args);
  const viewResult = await db.view(args.mapName, "default", { reduce: false });
  renderView(viewResult);
}
