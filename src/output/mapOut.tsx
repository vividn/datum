import React from "react";
import { Box, render, Text } from "ink";
import { EitherDocument, EitherPayload } from "../documentControl/DatumDocument";
import { DocumentViewResponse } from "nano";
import { Show } from "./output";
import Table from "ink-table";

type MapOutType = {
  view: DocumentViewResponse<unknown, EitherPayload>;
  show: Show;
};

type StringifiedRow = {
  key: string;
  value: string;
  id: string;
  doc?: string;
};

const MapOut: React.FC<MapOutType> = ({ view, show }) => {
  const stringifiedData = view.rows.map((row) => {
    const stringified: StringifiedRow = { key: row.key, value: String(row.value), id: row.id };
    if (row.doc !== undefined) {
      stringified.doc = String(row.doc);
    }
    return stringified
  });
  return <Table data={stringifiedData} skeleton={() => <React.Fragment />} />;
};

export default function ({ view, show }: MapOutType): void {
  render(<MapOut view={view} show={show} />);
}
