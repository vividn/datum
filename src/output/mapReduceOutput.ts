import Table from "easy-table";
import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { pullOutData } from "../utils/pullOutData";
import { JsonObject } from "../utils/utilityTypes";
import { MapRow } from "../views/DatumView";

export function mapReduceOutput(
  viewResponse: PouchDB.Query.Response<EitherPayload> | MapRow<any>[],
  showId?: boolean,
  hid?: boolean,
  columns?: string[],
): string {
  const rows = Array.isArray(viewResponse)
    ? viewResponse
    : ((viewResponse as PouchDB.Query.Response<EitherPayload>)
        .rows as MapRow<any>[]);
  const dataRows = rows.map((row) => {
    const keyValue: {
      key: unknown;
      value: unknown;
      id?: string;
      hid?: string;
    } & JsonObject = {
      key: JSON.stringify(row.key),
      value: JSON.stringify(row.value),
    };
    if (showId) {
      keyValue.id = row.id;
    }
    if (!row.doc) {
      return keyValue;
    }
    const { data, meta } = pullOutData(row.doc as EitherDocument);
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
