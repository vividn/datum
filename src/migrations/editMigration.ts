import { EitherPayload } from "../documentControl/DatumDocument";
import {
  asViewDb,
  MapFunction,
  StringifiedDatumView,
} from "../views/DatumView";
import { isCouchDbError } from "../errors";
import { editInTerminal } from "../utils/editInTerminal";
import { updateStrategies } from "../documentControl/combineData";
import { getMigrationId, getMigrationViewName } from "./migrations";
import { OutputArgs } from "../input/outputArgs";
import { insertDatumView } from "../views/insertDatumView";

const templateMigration = `(doc) => {
  data = doc.data
  if (data.condition === true) {
    data.condition = false;
    emit("overwrite", doc);
  }
}
`;

async function editWithExplanation(mapFn: string): Promise<string> {
  const divider = "////////// README //////////";
  const explanationString =
    divider +
    `
// emit(key, value) should be:
// emit('delete', null), emit('overwrite', completeNewDoc)
// or have the key be one of the updateMethods, and the value be a JSON of how to apply that method:
// ${Object.keys(updateStrategies).join(", ")}
// for example
// emit("merge", { field: "value to merge in" }
`;
  const beforeWithExplanation = mapFn + "\n\n\n" + explanationString;
  const editedWithExplanation = await editInTerminal(beforeWithExplanation);
  return editedWithExplanation.split(divider)[0].trim();
}

type baseMigrationType = {
  db: PouchDB.Database<EitherPayload>;
  migrationName: string;
  outputArgs?: OutputArgs;
};

type editMigrationType = baseMigrationType & {
  mapFn?: string | MapFunction;
};
export async function editMigration({
  db,
  migrationName,
  mapFn,
  outputArgs = {},
}: editMigrationType): Promise<void> {
  const viewDb = asViewDb(db);
  const viewName = getMigrationViewName(migrationName);

  let mapFnStr: string | undefined;
  if (mapFn) {
    mapFnStr = mapFn.toString();
  } else {
    const migrationId = getMigrationId(migrationName);
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
