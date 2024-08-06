import { EitherPayload } from "../documentControl/DatumDocument";
import {
  asViewDb,
  MapFunction,
  StringifiedDatumView,
} from "../views/DatumView";
import { isCouchDbError } from "../errors";
import { editInTerminal } from "../utils/editInTerminal";
import { updateStrategies } from "../documentControl/combineData";
import { getMigrationId, migrationName } from "./migrations";
import { OutputArgs } from "../input/outputArgs";
import { insertDatumView } from "../views/insertDatumView";

const templateMigration = `(doc) => {
  data = doc.data
  if (data.condition === true) {
    data.condition = false;
    emit(null, {op: "overwrite", data: doc});
  }
}
`;

async function editWithExplanation(mapFn: string): Promise<string> {
  const divider = "////////// README //////////";
  const explanationString =
    divider +
    `
// emit(key, value) should be used to apply a migration to a document.
// key: any -- provides sorting for the migration operations. Rows with the same key are processed in parallel.
// value: { op: MigrationOps, data: {}}
// op: is "delete" to delete the document, "overwrite" to replace the document with the data in value.data,
// or one of the updateMethods from the documentControl/combineData module.
// ${Object.keys(updateStrategies).join(", ")}
// for example
// emit(1, { op: "merge", data: { someKey: "someValue" }}
// will merge "someValue" into the document data at someKey
`;
  const beforeWithExplanation = mapFn + "\n\n\n" + explanationString;
  const editedWithExplanation = await editInTerminal(beforeWithExplanation);
  return editedWithExplanation.split(divider)[0].trim();
}

type baseMigrationType = {
  db: PouchDB.Database<EitherPayload>;
  name: string;
  outputArgs?: OutputArgs;
};

type editMigrationType = baseMigrationType & {
  mapFn?: string | MapFunction;
};
export async function editMigration({
  db,
  name,
  mapFn,
  outputArgs = {},
}: editMigrationType): Promise<void> {
  const viewDb = asViewDb(db);
  const viewName = migrationName(name);

  let mapFnStr: string | undefined;
  if (mapFn) {
    mapFnStr = mapFn.toString();
  } else {
    const migrationId = getMigrationId(name);
    try {
      const designDoc = await viewDb.get(migrationId);
      mapFnStr = designDoc.views[viewName]?.map;
    } catch (error) {
      if (
        !(
          isCouchDbError(error) && ["missing", "deleted"].includes(error.reason)
        )
      ) {
        throw error;
      }
    }
    mapFnStr = await editWithExplanation(mapFnStr ?? templateMigration);
  }

  if (mapFnStr === undefined) return;

  const migrationDatumView: StringifiedDatumView = {
    name: viewName,
    map: mapFnStr,
  };
  await insertDatumView({ db, datumView: migrationDatumView, outputArgs });
}
