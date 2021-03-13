import { DocumentScope, ViewDocument } from "nano";

const template_migration = `(doc) => {
  // Conditional to check if the document should be migrate
  if (doc.conditional === true) {
    // Make changes to document
    doc.conditional = false
    emit('replace', doc)
  }
}`

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
  const designDoc = await db.get("_design/migrate").catch(() => ({_id: "_design/migrate", views: {}})) as ViewDocument<{}>;
  
  if (mapFnStr === undefined) {
    mapFnStr = template_migration
    // TODO: interactive interface
  }

  designDoc.views[migrationName] = {map: mapFnStr}
  console.log(designDoc)
  await db.insert(designDoc)
};

exports.runMigration = ({ db, migrationName }: baseMigrationType) => {
  return;
};
