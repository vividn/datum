import { BaseDocControlArgs } from "../documentControl/base";
import { updateStrategies } from "../documentControl/combineData";
import { EitherDocument } from "../documentControl/DatumDocument";
import { deleteDoc } from "../documentControl/deleteDoc";
import { overwriteDoc } from "../documentControl/overwriteDoc";
import { updateDoc } from "../documentControl/updateDoc";
import { MigrationError } from "../errors";
import { MigrationMapRow } from "./migrations";

type MigrateOneType = {
  row: MigrationMapRow;
} & BaseDocControlArgs;
export async function migrateOne({
  row,
  db,
  outputArgs = {},
}: MigrateOneType): Promise<EitherDocument> {
  const {
    key,
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
