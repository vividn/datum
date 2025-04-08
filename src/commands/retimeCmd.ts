import { ArgumentParser } from "argparse";
import { connectDb } from "../auth/connectDb";
import { EitherDocument } from "../documentControl/DatumDocument";
import { updateDoc } from "../documentControl/updateDoc";
import { QuickIdArgs, quickIdArgs } from "../input/quickIdArg";
import { timeArgs, TimeArgs, handleTimeArgs } from "../input/timeArgs";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { quickId, _LAST_WITH_PROTECTION } from "../ids/quickId";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";

export const retimeCmdArgs = new ArgumentParser({
  description: "Change the occurrence time of an existing document.",
  prog: "datum retime",
  usage: `%(prog)s <quickId> [time options...]`,
  parents: [quickIdArgs, timeArgs, dbArgs, outputArgs],
});

export type RetimeCmdArgs = MainDatumArgs & TimeArgs & QuickIdArgs;

export async function retimeCmd(
  argsOrCli: RetimeCmdArgs | string | string[],
  preparsed?: Partial<RetimeCmdArgs>,
): Promise<EitherDocument[]> {
  const args = parseIfNeeded(retimeCmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);

  // Resolve quickId to document ID(s)
  const ids = await quickId(args.quickId ?? _LAST_WITH_PROTECTION, args);

  // Update lastDocs reference
  await updateLastDocsRef(db, ids);

  // Get the time from args
  const { time: occurTime } = handleTimeArgs(args);


  // Create payload with new occurTime
  const payload = { occurTime };

  // Update each document with the new occurrence time
  const updatedDocs = await Promise.all(
    ids.map((id) =>
      updateDoc({
        db,
        id,
        payload,
        updateStrategy: "update",
        outputArgs: args,
      }),
    ),
  );

  // Update lastDocs if needed
  const newIds = updatedDocs.map((doc) => doc._id);
  if (newIds !== ids) {
    await updateLastDocsRef(db, newIds);
  }

  return updatedDocs;
}
