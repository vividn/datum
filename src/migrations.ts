import { DocumentScope, ViewDocument, MaybeDocument, Document } from "nano";
const utils = require("./utils");

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

  const mapFn =
    mapFnStr ?? (await utils.editInTerminal(currentOrTemplate));
  if (mapFn === undefined) return;

  designDoc.views[migrationName] = { map: mapFn };
  await db.insert(designDoc);
};

exports.runMigration = async ({ db, migrationName }: baseMigrationType) => {
  const rows = (await db.view("migrate", migrationName)).rows
  rows.forEach(async (row) => {
    switch(row.key) {
      case "overwrite":
        await db.insert(row.value as Document)
        break;
      default:
        throw MigrationError
        break
    }
    
  })
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
