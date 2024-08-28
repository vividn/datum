import Table from "easy-table";
import { EitherPayload } from "../documentControl/DatumDocument";
import { pullOutData } from "../utils/pullOutData";

export function mapReduceOutput(
  viewResponse: PouchDB.Query.Response<EitherPayload>,
  showId?: boolean,
  hid?: boolean,
  columns?: string[],
): string {
  const dataRows = viewResponse.rows.map((row) => {
    const keyValue: {
      key: unknown;
      value: unknown;
      id?: string;
      hid?: string;
    } & Record<string, any> = {
      key: JSON.stringify(row.key),
      value: JSON.stringify(row.value),
    };
    if (showId) {
      keyValue.id = row.id;
    }
    if (!row.doc) {
      return keyValue;
    }
    const { data, meta } = pullOutData(row.doc);
    if (hid) {
      keyValue.hid = meta?.humanId?.slice(0, 6);
    }
    if (columns) {
      columns.forEach((col) => {
        const columnValue = data?.[col];
        if (columnValue !== undefined) {
          keyValue[col] = JSON.stringify(columnValue);
        }
      });
    }
    return keyValue;
  });

  return Table.print(dataRows);
}
