import { DocumentScope, ViewDocument } from "nano";
const tmp = require("tmp");
const fs = require("fs");
const child_process = require("child_process");

const template_migration = `(doc) => {
  // Conditional to check if the document should be migrate
  if (doc.conditional === true) {
    // Make changes to document
    doc.conditional = false
    emit('replace', doc)
  }
}`;

const migrationEditor = async (mapFn: string): Promise<string | undefined> => {
  const child_process = require("child_process");
  const editor = process.env.EDITOR || "vi";

  return new Promise((resolve, reject) => {
    tmp.file((err: Error, path: string) => {
      if (err) throw err;
      fs.writeFileSync(path, mapFn);

      const child = child_process.spawn(editor, [path], {
        stdio: "inherit",
      });
      child.on("exit", (code: number) => {
        if (code !== 0) resolve(undefined);
        const newMapFn = fs.readFileSync(path, "utf8");
        resolve(newMapFn);
      });
    });
  });
};

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
  
  const mapFn = mapFnStr ?? (await migrationEditor(currentOrTemplate)) ?? "nothing";
  if (mapFn === undefined) return;

  designDoc.views[migrationName] = { map: mapFn };
  await db.insert(designDoc);
};

exports.runMigration = ({ db, migrationName }: baseMigrationType) => {
  return;
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
