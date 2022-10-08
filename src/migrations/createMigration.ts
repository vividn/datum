import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { asViewDb, ViewPayload } from "../views/viewDocument";
import { isCouchDbError } from "../errors";
import { editInTerminal } from "../utils/editInTerminal";
import { updateStrategies } from "../documentControl/combineData";
import { addDoc } from "../documentControl/addDoc";
import { Show } from "../output/output";

const template_migration = `(doc) => {
  if (doc.trueBeforeFalseAfter === true) {
    emit(...,...)
  }
  // emit should be:
  // emit('delete', null), emit('overwrite', completeNewDoc)
  // or have the key be one of the updateMethods, and the value be a JSON of how to apply that method:
  // ${Object.keys(updateStrategies)}
`;

type baseMigrationType = {
  db: DocumentScope<EitherPayload>;
  migrationName: string;
};

type createMigrationType = baseMigrationType & {
  mapFnStr?: string;
};
export async function createMigration({
  db,
  migrationName,
  mapFnStr,
}: createMigrationType): Promise<void> {
  const viewDb = asViewDb(db);
  const fullMigrationId = `_design/migrate_${migrationName}`;
  let designDoc: ViewPayload;
  try {
    designDoc = await viewDb.get(fullMigrationId);
  } catch (error) {
    if (!(isCouchDbError(error) && error.reason in ["missing", "deleted"])) {
      throw error;
    }
    designDoc = {
      _id: fullMigrationId,
      views: {},
      meta: {}
    };
  }

  const currentOrTemplate = (designDoc.views["migration"]?.map ??
    template_migration) as string;

  const mapFn = mapFnStr ?? (await editInTerminal(currentOrTemplate));
  if (mapFn === undefined) return;

  designDoc.views["migration"] = { map: mapFn };
  await addDoc({db, payload: designDoc, show: Show.Minimal, conflictStrategy: "overwrite"});
}