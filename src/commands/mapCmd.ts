import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { subHumanIdView } from "../views/datumViews/humanId";

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

export async function mapCmd(_args: MapCmdArgs): Promise<void> {
  console.log(subHumanIdView.map.toString());
}
