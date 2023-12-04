import { render } from "ink";
import { EitherPayload } from "../documentControl/DatumDocument";
import Table from "ink-table";
import React from "react";

const CustomSkeleton = ({ children: _children }: React.PropsWithChildren) => (
  <React.Fragment />
);

export function renderView(
  viewResponse: PouchDB.Query.Response<EitherPayload>,
  showId?: boolean,
  hid?: boolean,
): void {
  const data = viewResponse.rows.map((row) => {
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
  render(<Table data={data} skeleton={CustomSkeleton} />);
}
