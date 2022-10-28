import { BaseDatumArgs } from "../input/baseYargs";
import { Argv } from "yargs";

export const command = "reduce <mapName> [groupLevel]";
export const desc = "display a reduction of a map";

export type ReduceCmdArgs = BaseDatumArgs & {
  mapName: string;
  groupLevel?: number;
  reduce?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.positional("mapName");
}
