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
reduceArgs.add_argument("--group-level", "-g", {
  help: "how far to group the key arrays when reducing",
  type: "int",
  dest: "groupLevel",
});

export const reduceCmdArgs = new ArgumentParser({
  description: "display a reduction of a map",
  prog: "dtm red[uce]",
  usage: `%(prog)s <mapName> [start] [end]
  %(prog)s --group-level <level> <mapName> [start] [end]`,
  parents: [reduceArgs, dbArgs, outputArgs],
});

export type ReduceCmdArgs = MapCmdArgs & {
  groupLevel?: number;
};

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
