import Table from "easy-table";
import { EitherPayload } from "../documentControl/DatumDocument";

export function mapReduceOutput(
  viewResponse: PouchDB.Query.Response<EitherPayload>,
  showId?: boolean,
  hid?: boolean,
): string {
  const dataRows = viewResponse.rows.map((row) => {
    const keyValue: { key: any; value: any; id?: string; hid?: string } = {
      key: JSON.stringify(row.key),
      value: JSON.stringify(row.value),
    };
    if (showId) {
      keyValue.id = row.id;
    }
    if (hid) {
      keyValue.hid = row.doc?.meta?.humanId?.slice(0, 6);
    }
    return keyValue;
  });

  return Table.print(dataRows);
}
