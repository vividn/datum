import { asViewDb, ViewDocument, ViewPayload } from "../views/viewDocument";
import { EitherDocument, EitherPayload } from "../documentControl/DatumDocument";
import { DocumentScope } from "nano";
import { isCouchDbError } from "../errors";
import { editInTerminal } from "../utils/editInTerminal";




export async function runMigration({
  db,
  migrationName,
}: baseMigrationType): Promise<{ pass: string[]; fail: [string, Error][] }> {
  const rows = (await db.view("migrate", migrationName)).rows;
  const updateResults = await Promise.allSettled(
    rows.map(async (row) => {
      switch (row.key) {
        case "overwrite": {
          const newDocument = row.value as MaybeDocument;
          try {
            await db.insert(row.value as Document);
            return Promise.resolve(newDocument._id);
          } catch (err) {
            return Promise.reject([row.id, err]);
          }
        }
        default: {
          return Promise.reject([
            row.id,
            new MigrationError("unrecognized operation"),
          ]);
        }
      }
    })
  );

  return updateResults.reduce(
    (retVal, result) => {
      if (result.status === "fulfilled") {
        retVal.pass.push(result.value as string);
      } else {
        retVal.fail.push(result.reason as [string, Error]);
      }
      return retVal;
    },
    { pass: [] as string[], fail: [] as [string, Error][] }
  );
}

if (require.main === module) {
  testNano.db.create("test").catch(pass).then(pass);
  const db = testNano.use("test");
  exports.createMigration({ db: db, migrationName: "test-migration" });
}
