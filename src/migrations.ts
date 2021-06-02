import { DocumentScope, ViewDocument, MaybeDocument, Document } from "nano";
import { MigrationError } from "./errors";
import editInTerminal from "./utils/editInTerminal";

const template_migration = `(doc) => {
  // Conditional to check if the document should be migrate
  if (doc.conditional === true) {
    // Make changes to document
    doc.conditional = false
    emit('replace', doc)
  }
}`;

type baseMigrationType = {
  db: DocumentScope<{}>;
  migrationName: string;
};

type createMigrationType = baseMigrationType & {
  mapFnStr?: string;
};
exports.createMigration = async ({
  db,
  migrationName,
  mapFnStr,
}: createMigrationType) => {
  const designDoc = (await db
    .get("_design/migrate")
    .catch(() => ({ _id: "_design/migrate", views: {} }))) as ViewDocument<{}>;

  const currentOrTemplate = (designDoc.views[migrationName]?.map ??
    template_migration) as string;

  const mapFn = mapFnStr ?? (await editInTerminal(currentOrTemplate));
  if (mapFn === undefined) return;

  designDoc.views[migrationName] = { map: mapFn };
  await db.insert(designDoc);
};

type overwriteMigrationValueType = {

}

export const runMigration = async ({ db, migrationName }: baseMigrationType): Promise<string[]> => {
  const rows = (await db.view("migrate", migrationName)).rows;
  const updateResults = await Promise.allSettled(rows.map(async (row) => {
    switch (row.key) {
      case "overwrite":
        const newDocument = row.value as MaybeDocument
        try {
          await db.insert(row.value as Document);
          return Promise.resolve(newDocument._id)
        } catch (err) {
          return Promise.reject([row.id, err])
        }
      default:
        return Promise.reject([row.id, new MigrationError("unrecognized operation")]);
    }
  }))
  
  updateResults.reduce((retVal, result) => {if (result.status == "fulfilled") {
    retVal.pass.push(result.value)
  } else {
    retVal.fail.push(result.reason as [string, Error])
  }}, {pass: [] as string[], fail: [] as [string, Error][]}
};

if (require.main === module) {
  const nano = require("nano")("http://admin:password@localhost:5983");
  nano.db
    .create("test")
    .catch(() => {})
    .then(() => {});
  const db = nano.use("test");
  exports.createMigration({ db: db, migrationName: "test-migration" });
}
