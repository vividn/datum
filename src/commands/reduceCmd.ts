import { Argv } from "yargs";
import { renderView } from "../output/renderView";
import { mapCmd, MapCmdArgs, mapCmdYargs } from "./mapCmd";
import { EitherPayload } from "../documentControl/DatumDocument";
import { Show } from "../input/outputArgs";

export const command = [
  "reduce <mapName> [start] [end]",
  "red <mapName> [start] [end]",
];
export const desc = "display a reduction of a map";

export type ReduceCmdArgs = MapCmdArgs & {
  groupLevel?: number;
};

export function builder(yargs: Argv): Argv {
  return mapCmdYargs(yargs).options({
    groupLevel: {
      describe: "how far to group the key arrays when reducing",
      type: "number",
      alias: "g",
    },
  });
}

export async function reduceCmd(
  args: ReduceCmdArgs,
): Promise<PouchDB.Query.Response<EitherPayload>> {
  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";

  // mock _count reduce function on the _all_docs list
  if (useAllDocs) {
    const mapResult = await mapCmd({
      ...args,
      reduce: false,
      show: Show.None,
    });
    const mockReduceResult = {
      rows: [{ key: null, value: mapResult.rows.length, id: "" }],
      total_rows: mapResult.total_rows,
      offset: mapResult.offset,
    };
    if (args.show !== Show.None) {
      renderView(mockReduceResult);
    }
    return mockReduceResult;
  }

  const groupParams = args.groupLevel
    ? { group_level: args.groupLevel, group: true }
    : {};
  return await mapCmd({
    ...args,
    reduce: true,
    params: {
      ...groupParams,
      ...(args.params ?? {}),
    },
  });
}
