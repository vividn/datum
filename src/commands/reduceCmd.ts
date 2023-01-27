import { Argv } from "yargs";
import { connectDb } from "../auth/connectDb";
import { renderView } from "../output/renderView";
import { mapCmd, MapCmdArgs, mapCmdYargs } from "./mapCmd";
import { DocumentViewResponse } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { Show } from "../input/outputArgs";

export const command = "reduce <mapName> [start] [end]";
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
  args: ReduceCmdArgs
): Promise<DocumentViewResponse<unknown, EitherPayload<unknown>>> {
  const db = await connectDb(args);

  const useAllDocs = args.mapName === "_all_docs" || args.mapName === "_all";

  // mock _count reduce function on the _all_docs list
  if (useAllDocs) {
    const mapResult = await mapCmd({
      ...args,
      reduce: undefined,
      show: Show.None,
    });
    const mockReduceResult = {
      rows: [{ key: null, value: mapResult.rows.length, id: "" }],
      total_rows: mapResult.total_rows,
      offset: mapResult.offset,
    };
    renderView(mockReduceResult);
    return mockReduceResult;
  }
  return await mapCmd({
    ...args,
    reduce: true,
  });
}
