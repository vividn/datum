import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { asViewDb, MapFunction, ViewPayload } from "../views/viewDocument";
import { isCouchDbError } from "../errors";
import { editInTerminal } from "../utils/editInTerminal";
import { updateStrategies } from "../documentControl/combineData";
import { addDoc } from "../documentControl/addDoc";
import { Show } from "../output/output";
import { getMigrationId } from "./migrations";

const templateMigration = `(doc) => {
  if (doc.condition === true) {
    doc.condition = false
    emit("overwrite",doc)
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
  const migrationId = getMigrationId(migrationName);
  let designDoc: ViewPayload;
  try {
    designDoc = await viewDb.get(migrationId);
  } catch (error) {
    if (
      !(isCouchDbError(error) && ["missing", "deleted"].includes(error.reason))
    ) {
      throw error;
    }
    designDoc = {
      _id: migrationId,
      views: {},
      meta: {},
    };
  }

  const currentOrTemplate = (designDoc.views["migration"]?.map ??
    templateMigration) as string;

  const mapFnStr = mapFn
    ? mapFn.toString()
    : await editWithExplanation(currentOrTemplate);
  if (mapFnStr === undefined) return;

  designDoc.views["migration"] = { map: mapFnStr };
  await addDoc({
    db,
    payload: designDoc,
    show: Show.Minimal,
    conflictStrategy: "overwrite",
  });
}
