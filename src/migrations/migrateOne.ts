import { BaseDocControlArgs } from "../documentControl/base.js";
import { updateStrategies } from "../documentControl/combineData.js";
import { EitherDocument } from "../documentControl/DatumDocument.js";
import { deleteDoc } from "../documentControl/deleteDoc.js";
import { overwriteDoc } from "../documentControl/overwriteDoc.js";
import { updateDoc } from "../documentControl/updateDoc.js";
import { MigrationError } from "../errors.js";
import { MigrationMapRow } from "./migrations.js";

type MigrateOneType = {
  row: MigrationMapRow;
} & BaseDocControlArgs;
export async function migrateOne({
  row,
  db,
  outputArgs = {},
}: MigrateOneType): Promise<EitherDocument> {
  const {
    id,
    value: { op, data },
  } = row;
  switch (true) {
    case op === "overwrite":
      return await overwriteDoc({ id, payload: data, db, outputArgs });

    case op === "delete":
      return await deleteDoc({ id, db, outputArgs });

    case Object.keys(updateStrategies).includes(op):
      return await updateDoc({
        id,
        payload: data,
        updateStrategy: op,
        db,
        outputArgs,
      });

    default:
      throw new MigrationError(`update operator ${op} not recognized`);
  }
}
