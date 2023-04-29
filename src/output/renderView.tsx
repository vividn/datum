import { render } from "ink";
import { EitherPayload } from "../documentControl/DatumDocument";
import Table from "ink-table";
import React from "react";

const CustomSkeleton = ({ children: _children }: React.PropsWithChildren) => (
  <React.Fragment />
);

export function renderView(
  viewResponse: PouchDB.Query.Response<EitherPayload>
): void {
  const data = viewResponse.rows.map((row) => ({
    key: row.key,
    value: row.value,
    id: row.doc?.meta?.humanId?.slice(0,6) ?? row.id,
  }));
  render(<Table data={data} skeleton={CustomSkeleton} />);
}
