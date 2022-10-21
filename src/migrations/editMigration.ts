import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { asViewDb, MapFunction, ViewPayload } from "../views/viewDocument";
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

type editMigrationType = baseMigrationType & {
  mapFn?: string | MapFunction;
};
export async function editMigration({
  db,
  migrationName,
  mapFn,
}: editMigrationType): Promise<void> {
  const viewDb = asViewDb(db);
  const fullMigrationId = `_design/migrate_${migrationName}`;
  let designDoc: ViewPayload;
  try {
    designDoc = await viewDb.get(fullMigrationId);
  } catch (error) {
    if (!(isCouchDbError(error) && ["missing", "deleted"].includes(error.reason))) {
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

  const mapFnStr = mapFn ? mapFn.toString() : (await editInTerminal(currentOrTemplate));
  if (mapFnStr === undefined) return;

  designDoc.views["migration"] = { map: mapFnStr };
  await addDoc({db, payload: designDoc, show: Show.Minimal, conflictStrategy: "overwrite"});
}