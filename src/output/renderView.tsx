import {render} from 'ink';
import { DocumentViewResponse } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import Table from "ink-table";
import React from 'react';

const CustomSkeleton = ({ children }: React.PropsWithChildren<{}>) => (
  <React.Fragment />
);

export function renderView(viewResponse: DocumentViewResponse<any, EitherPayload>): void {
  const data = viewResponse.rows.map((row) => ({key: row.key, value: row.value}));
  render(<Table data={data} skeleton={CustomSkeleton} />);
}