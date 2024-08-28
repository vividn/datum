import { mapArgs, mapCmd, MapCmdArgs } from "./mapCmd";
import { EitherPayload } from "../documentControl/DatumDocument";
import { outputArgs, Show } from "../input/outputArgs";
import { ArgumentParser } from "argparse";
import { dbArgs } from "../input/dbArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { mapReduceOutput } from "../output/mapReduceOutput";

export const reduceArgs = new ArgumentParser({
  add_help: false,
  parents: [mapArgs],
});

export const reduceCmdArgs = new ArgumentParser({
  description: "display a reduction of a map",
  prog: "datum red[uce]",
  usage: `%(prog)s <mapName> [start] [end]
  %(prog)s -g <groupLevel> <mapName> [start] [end]`,
  parents: [reduceArgs, dbArgs, outputArgs],
});

export type ReduceCmdArgs = MapCmdArgs;

export async function reduceCmd(
  args: ReduceCmdArgs | string | string[],
  preparsed?: Partial<ReduceCmdArgs>,
): Promise<PouchDB.Query.Response<EitherPayload>> {
  args = parseIfNeeded(reduceCmdArgs, args, preparsed);
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
      const output = mapReduceOutput(mockReduceResult);
      console.log(output);
    }
    return mockReduceResult;
  }
  return await mapCmd({
    ...args,
    reduce: true,
  });
}
