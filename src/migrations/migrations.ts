// import { DocumentScope, ViewDocument, MaybeDocument, Document } from "nano";
// import { isCouchDbError, MigrationError } from "./errors";
// import editInTerminal from "./utils/editInTerminal";
// import pass from "./utils/pass";
// import { testNano } from "../test/test-utils";
// import { GenericObject } from "./GenericObject";
// import { asViewDb } from "./views/getViewDoc";
//
// const template_migration = `(doc) => {
//   // Conditional to check if the document should be migrate
//   if (doc.conditional === true) {
//     // Make changes to document
//     doc.conditional = false
//     emit('replace', doc)
//   }
// }`;
//
// type baseMigrationType = {
//   db: DocumentScope<GenericObject>;
//   migrationName: string;
// };
//
// type createMigrationType = baseMigrationType & {
//   mapFnStr?: string;
// };
// export async function createMigration({
//   db,
//   migrationName,
//   mapFnStr,
// }: createMigrationType): Promise<void> {
//   const viewDb = asViewDb(db);
//   let designDoc: ViewDocument<GenericObject>;
//   try {
//     designDoc = await viewDb.get("_design/migrate");
//   } catch (error) {
//     if (!(isCouchDbError(error) && error.reason in ["missing", "deleted"])) {
//       throw error;
//     }
//     designDoc = {
//       _id: "_design/migrate",
//       views: {},
//     };
//   }
//
//   const currentOrTemplate = (designDoc.views[migrationName]?.map ??
//     template_migration) as string;
//
//   const mapFn = mapFnStr ?? (await editInTerminal(currentOrTemplate));
//   if (mapFn === undefined) return;
//
//   designDoc.views[migrationName] = { map: mapFn };
//   await db.insert(designDoc);
// }
//
// export async function runMigration({
//   db,
//   migrationName,
// }: baseMigrationType): Promise<{ pass: string[]; fail: [string, Error][] }> {
//   const rows = (await db.view("migrate", migrationName)).rows;
//   const updateResults = await Promise.allSettled(
//     rows.map(async (row) => {
//       switch (row.key) {
//         case "overwrite": {
//           const newDocument = row.value as MaybeDocument;
//           try {
//             await db.insert(row.value as Document);
//             return Promise.resolve(newDocument._id);
//           } catch (err) {
//             return Promise.reject([row.id, err]);
//           }
//         }
//         default: {
//           return Promise.reject([
//             row.id,
//             new MigrationError("unrecognized operation"),
//           ]);
//         }
//       }
//     })
//   );
//
//   return updateResults.reduce(
//     (retVal, result) => {
//       if (result.status === "fulfilled") {
//         retVal.pass.push(result.value as string);
//       } else {
//         retVal.fail.push(result.reason as [string, Error]);
//       }
//       return retVal;
//     },
//     { pass: [] as string[], fail: [] as [string, Error][] }
//   );
// }
//
// if (require.main === module) {
//   testNano.db.create("test").catch(pass).then(pass);
//   const db = testNano.use("test");
//   exports.createMigration({ db: db, migrationName: "test-migration" });
// }
