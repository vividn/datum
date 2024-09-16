import { ArgumentParser } from "argparse";
import { FieldArgs } from "../input/fieldArgs";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { checkState, StateChangeError, StateErrorSummary } from "../state/checkState";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";
import { stateChangeView } from "../views/datumViews";

export const checkArgs = new ArgumentParser({
  add_help: false,
  parents: [dbArgs, outputArgs],
});
checkArgs.add_argument("field", {
  help: "the data to check. Defaults to check all fields",
  nargs: "?",
});
checkArgs.add_argument("--fix", {
  help: "attempt to fix the data where possible",
  action: "store_true",
});

export const checkCmdArgs = new ArgumentParser({
  description: "Check for problems in the data",
  prog: "datum check",
  usage: `%(prog)s [field]`,
  parents: [checkArgs],
});

export type CheckCmdArgs = FieldArgs &
  MainDatumArgs & {
    fix?: boolean;
  };

export async function checkCmd(
  argsOrCli: CheckCmdArgs | string | string[],
  preparsed?: Partial<CheckCmdArgs>,
): Promise<StateErrorSummary> {
  const args = parseIfNeeded(checkCmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);
  const fields = args.field?.split(",") ?? (await allCheckFields(db)); // TODO: get all fields
  const allFieldErrors = await Promise.allSettled(
    fields.map(async (field) => {
      const fieldErrors = await checkState({
        db,
        field,
        fix: args.fix,
        outputArgs: args,
        failOnError: false,
      });
      if (!fieldErrors.ok) {
        const errorMessage =
          `${field}: \n` +
          fieldErrors.errors.map((error) => error.message).join("\n") +
          "\n";
        console.error(errorMessage);
      }
      return fieldErrors;
    }),
  );
  const allErrors = allFieldErrors.reduce(
    (accum, fieldErrors) => {
      if (fieldErrors.status === "fulfilled") {
        if (fieldErrors.value.ok === false) {
          return {
            ok: false,
            errors: accum.errors.concat(fieldErrors.value.errors),
          };
      }
        return accum;
      }
      return {
        ok: false,
        errors: accum.errors.concat(fieldErrors.reason),
      };
    },
    { ok: true, errors: [] as StateChangeError[] },
  );
  return allErrors;
}

async function allCheckFields(db: PouchDB.Database): Promise<string[]> {
  const fields = await db.query(stateChangeView.name, {
    reduce: true,
    group: true,
    group_level: 1,
  });
  const allFields = fields.rows.map((row) => row.key[0]);
  return allFields;
}
